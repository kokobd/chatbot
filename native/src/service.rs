use bytes::Bytes;
use napi_derive::napi;
use thiserror::Error;

use crate::application::file_upload::{FileUploadError, FileUploadService, UploadResult};
use crate::application::iap_authentication::{IapAuthenticationError, IapAuthenticationService};
use crate::application::iap_identity::{
    AuthenticatedIdentity, IapIdentityProvider, IapIdentityProviderError, IapRequestEvidence,
};
use crate::infrastructure::gcs_object_storage::{GcsObjectStorage, GcsObjectStorageError};
use crate::infrastructure::iap_google_identity::GoogleIapIdentityProvider;
use crate::infrastructure::iap_test_identity::TestIapIdentityProvider;
use std::sync::Arc;

#[napi]
pub struct Service {
    authentication: IapAuthenticationService,
    file_upload: FileUploadService<GcsObjectStorage>,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Authentication(#[from] IapAuthenticationError),
    #[error(transparent)]
    Configuration(#[from] GcsObjectStorageError),
    #[error(transparent)]
    Upload(#[from] FileUploadError),
}

impl Service {
    pub fn new(
        authentication: IapAuthenticationService,
        file_upload: FileUploadService<GcsObjectStorage>,
    ) -> Self {
        Self {
            authentication,
            file_upload,
        }
    }

    pub async fn authenticate_iap(
        &self,
        evidence: &IapRequestEvidence,
    ) -> Result<Option<AuthenticatedIdentity>, ServiceError> {
        self.authentication
            .authenticate(evidence)
            .await
            .map_err(ServiceError::from)
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

pub async fn create_service() -> Result<Service, ServiceError> {
    install_rustls_provider();

    let provider = create_iap_provider()
        .map_err(|error| ServiceError::Authentication(IapAuthenticationError::Provider(error)))?;
    let authentication = IapAuthenticationService::new(provider);
    let storage = GcsObjectStorage::new_from_env().await?;

    Ok(Service::new(
        authentication,
        FileUploadService::new(storage),
    ))
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn create_iap_provider() -> Result<Arc<dyn IapIdentityProvider>, IapIdentityProviderError> {
    let provider = std::env::var("IAP_AUTH_PROVIDER").unwrap_or_else(|_| "google".to_string());

    match provider.as_str() {
        "google" => Ok(Arc::new(GoogleIapIdentityProvider::from_env()?)),
        "test" => {
            if is_production_environment() {
                return Err(IapIdentityProviderError::Configuration(
                    "IAP_AUTH_PROVIDER=test is not allowed in production".to_string(),
                ));
            }

            Ok(Arc::new(TestIapIdentityProvider))
        }
        value => Err(IapIdentityProviderError::Configuration(format!(
            "unsupported IAP_AUTH_PROVIDER value: {value}"
        ))),
    }
}

fn is_production_environment() -> bool {
    matches!(std::env::var("NODE_ENV").as_deref(), Ok("production"))
        || matches!(std::env::var("VERCEL_ENV").as_deref(), Ok("production"))
        || std::env::var_os("K_SERVICE").is_some()
}
