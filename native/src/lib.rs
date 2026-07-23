use bytes::Bytes;
use google_cloud_storage::client::Storage;
use napi::bindgen_prelude::Buffer;
use napi::{Error, Result, Status};
use napi_derive::napi;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use tokio::sync::OnceCell;
use uuid::Uuid;

static STORAGE: OnceCell<Storage> = OnceCell::const_new();
const PATH_SEGMENT_ENCODE_SET: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

#[napi(object)]
pub struct UploadResult {
    pub url: String,
    pub pathname: String,
    #[napi(js_name = "contentType")]
    pub content_type: String,
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| {
        Error::new(
            Status::InvalidArg,
            format!("{name} must be configured for GCS uploads"),
        )
    })
}

fn safe_filename(filename: &str) -> String {
    let sanitized = filename
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "upload".to_string()
    } else {
        sanitized
    }
}

fn object_name(filename: &str) -> String {
    format!("uploads/{}/{}", Uuid::new_v4(), safe_filename(filename))
}

fn public_url(bucket: &str, object: &str) -> String {
    let encoded_object = object
        .split('/')
        .map(|part| utf8_percent_encode(part, PATH_SEGMENT_ENCODE_SET).to_string())
        .collect::<Vec<_>>()
        .join("/");

    format!("https://storage.googleapis.com/{bucket}/{encoded_object}")
}

#[napi(js_name = "uploadObject")]
pub async fn upload_object(
    data: Buffer,
    filename: String,
    content_type: String,
) -> Result<UploadResult> {
    let bucket = required_env("GCS_BUCKET")?;
    let object = object_name(&filename);
    let client = STORAGE
        .get_or_try_init(|| async {
            Storage::builder().build().await.map_err(|error| {
                Error::new(
                    Status::GenericFailure,
                    format!("GCS client setup failed: {error}"),
                )
            })
        })
        .await?;

    client
        .write_object(
            format!("projects/_/buckets/{bucket}"),
            object.clone(),
            Bytes::from(data.to_vec()),
        )
        .set_content_type(content_type.clone())
        .send_buffered()
        .await
        .map_err(|error| {
            Error::new(
                Status::GenericFailure,
                format!("GCS upload failed: {error}"),
            )
        })?;

    Ok(UploadResult {
        url: public_url(&bucket, &object),
        pathname: safe_filename(&filename),
        content_type,
    })
}

#[cfg(test)]
mod tests {
    use super::{object_name, public_url, safe_filename};

    #[test]
    fn sanitizes_filenames() {
        assert_eq!(safe_filename("my image?.png"), "my_image_.png");
        assert_eq!(safe_filename(""), "upload");
    }

    #[test]
    fn creates_unique_upload_paths() {
        let first = object_name("photo.png");
        let second = object_name("photo.png");

        assert_ne!(first, second);
        assert!(first.starts_with("uploads/"));
        assert!(first.ends_with("/photo.png"));
    }

    #[test]
    fn encodes_public_url_path_segments() {
        assert_eq!(
            public_url("bucket", "uploads/id/file name#.png"),
            "https://storage.googleapis.com/bucket/uploads/id/file%20name%23.png"
        );
    }
}
