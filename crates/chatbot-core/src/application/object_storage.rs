use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ObjectStorageError {
    #[error("storage configuration error: {0}")]
    Configuration(String),
    #[error("storage provider error: {0}")]
    Provider(String),
}

#[async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn put_object(
        &self,
        object_name: &str,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), ObjectStorageError>;

    fn public_url(&self, object_name: &str) -> String;
}
