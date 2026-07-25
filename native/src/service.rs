use bytes::Bytes;
use napi_derive::napi;
use thiserror::Error;

use crate::application::file_upload::{FileUploadError, FileUploadService, UploadResult};
use crate::infrastructure::gcs_object_storage::{GcsObjectStorage, GcsObjectStorageError};

#[napi]
pub struct Service {
    file_upload: FileUploadService<GcsObjectStorage>,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Configuration(#[from] GcsObjectStorageError),
    #[error(transparent)]
    Upload(#[from] FileUploadError),
}

impl Service {
    pub async fn new() -> Result<Self, ServiceError> {
        let storage = GcsObjectStorage::new_from_env().await?;
        Ok(Self {
            file_upload: FileUploadService::new(storage),
        })
    }

    pub async fn upload_object(
        &self,
        data: Bytes,
        filename: String,
        content_type: String,
    ) -> Result<UploadResult, ServiceError> {
        self.file_upload
            .upload(data, filename, content_type)
            .await
            .map_err(ServiceError::from)
    }
}
