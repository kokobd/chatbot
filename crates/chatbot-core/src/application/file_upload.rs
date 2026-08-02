use super::object_storage::{ObjectStorage, ObjectStorageError};
use bytes::Bytes;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum FileUploadError {
    #[error(transparent)]
    Storage(#[from] ObjectStorageError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadResult {
    pub url: String,
    pub pathname: String,
    pub content_type: String,
}

pub struct FileUploadService<S> {
    storage: S,
}

impl<S> FileUploadService<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<S> FileUploadService<S>
where
    S: ObjectStorage,
{
    pub async fn upload(
        &self,
        data: Bytes,
        filename: String,
        content_type: String,
    ) -> Result<UploadResult, FileUploadError> {
        let pathname = safe_filename(&filename);
        let object_name = object_name(&pathname);

        self.storage
            .put_object(&object_name, data, &content_type)
            .await?;

        Ok(UploadResult {
            url: self.storage.public_url(&object_name),
            pathname,
            content_type,
        })
    }
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
    format!("uploads/{}/{}", Uuid::new_v4(), filename)
}

#[cfg(test)]
mod tests {
    use super::{safe_filename, FileUploadService};
    use crate::application::object_storage::{ObjectStorage, ObjectStorageError};
    use async_trait::async_trait;
    use bytes::Bytes;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeObjectStorage {
        uploaded: Arc<Mutex<Vec<(String, Vec<u8>, String)>>>,
    }

    #[async_trait]
    impl ObjectStorage for FakeObjectStorage {
        async fn put_object(
            &self,
            object_name: &str,
            data: Bytes,
            content_type: &str,
        ) -> Result<(), ObjectStorageError> {
            self.uploaded.lock().unwrap().push((
                object_name.to_string(),
                data.to_vec(),
                content_type.to_string(),
            ));
            Ok(())
        }

        fn public_url(&self, object_name: &str) -> String {
            format!("https://example.test/{object_name}")
        }
    }

    #[tokio::test]
    async fn uploads_with_a_fake_object_storage() {
        let storage = FakeObjectStorage::default();
        let uploaded = Arc::clone(&storage.uploaded);
        let service = FileUploadService::new(storage);

        let result = service
            .upload(
                Bytes::from_static(b"png data"),
                "my image?.png".to_string(),
                "image/png".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(result.pathname, "my_image_.png");
        assert_eq!(result.content_type, "image/png");
        assert!(result.url.starts_with("https://example.test/uploads/"));

        let uploaded = uploaded.lock().unwrap();
        assert_eq!(uploaded.len(), 1);
        assert!(uploaded[0].0.ends_with("/my_image_.png"));
        assert_eq!(uploaded[0].1, b"png data");
        assert_eq!(uploaded[0].2, "image/png");
    }

    #[test]
    fn sanitizes_filenames() {
        assert_eq!(safe_filename("my image?.png"), "my_image_.png");
        assert_eq!(safe_filename(""), "upload");
    }
}
