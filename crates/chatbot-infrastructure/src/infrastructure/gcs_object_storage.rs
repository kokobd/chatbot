use async_trait::async_trait;
use bytes::Bytes;
use google_cloud_storage::client::Storage;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::env;
use thiserror::Error;

use crate::application::object_storage::{ObjectStorage, ObjectStorageError};

const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

#[derive(Debug, Error)]
pub enum GcsObjectStorageError {
    #[error("GCS_BUCKET must be configured for GCS uploads")]
    MissingBucket,
    #[error("GCS_BUCKET must not be empty")]
    EmptyBucket,
    #[error("GCS client setup failed: {0}")]
    Client(String),
}

pub struct GcsObjectStorage {
    bucket: String,
    client: Storage,
}

impl GcsObjectStorage {
    pub async fn new(bucket: String) -> Result<Self, GcsObjectStorageError> {
        if bucket.trim().is_empty() {
            return Err(GcsObjectStorageError::EmptyBucket);
        }

        let client = Storage::builder()
            .build()
            .await
            .map_err(|error| GcsObjectStorageError::Client(error.to_string()))?;

        Ok(Self { bucket, client })
    }

    pub async fn new_from_env() -> Result<Self, GcsObjectStorageError> {
        let bucket = env::var("GCS_BUCKET").map_err(|_| GcsObjectStorageError::MissingBucket)?;
        Self::new(bucket).await
    }

    fn public_url_for(bucket: &str, object_name: &str) -> String {
        let encoded_object = object_name
            .split('/')
            .map(|part| utf8_percent_encode(part, PATH_SEGMENT_ENCODE_SET).to_string())
            .collect::<Vec<_>>()
            .join("/");

        format!("https://storage.googleapis.com/{bucket}/{encoded_object}")
    }
}

#[async_trait]
impl ObjectStorage for GcsObjectStorage {
    async fn put_object(
        &self,
        object_name: &str,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), ObjectStorageError> {
        self.client
            .write_object(
                format!("projects/_/buckets/{}", self.bucket),
                object_name.to_string(),
                data,
            )
            .set_content_type(content_type.to_string())
            .send_buffered()
            .await
            .map_err(|error| ObjectStorageError::Provider(error.to_string()))?;

        Ok(())
    }

    fn public_url(&self, object_name: &str) -> String {
        Self::public_url_for(&self.bucket, object_name)
    }
}

#[cfg(test)]
mod tests {
    use super::GcsObjectStorage;

    #[test]
    fn encodes_public_url_path_segments() {
        assert_eq!(
            GcsObjectStorage::public_url_for("bucket", "uploads/id/file name#.png"),
            "https://storage.googleapis.com/bucket/uploads/id/file%20name%23.png"
        );
    }
}
