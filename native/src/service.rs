use bytes::Bytes;
use napi_derive::napi;
use thiserror::Error;

use crate::application::artifact_service::{ArtifactService, ArtifactServiceError};
use crate::application::chat_service::{ChatService, ChatServiceError};
use crate::application::file_upload::{FileUploadError, FileUploadService, UploadResult};
use crate::application::iap_authentication::{IapAuthenticationError, IapAuthenticationService};
use crate::application::iap_identity::{
    AuthenticatedIdentity, IapIdentityProvider, IapIdentityProviderError, IapRequestEvidence,
};
use crate::application::message_service::{MessageService, MessageServiceError};
use crate::application::repository::{
    ArtifactRepository, ChatRepository, MessageRepository, UserRepository,
};
use crate::application::user_service::{UserService, UserServiceError};
use crate::infrastructure::firestore_artifact_repository::FirestoreArtifactRepository;
use crate::infrastructure::firestore_chat_repository::FirestoreChatRepository;
use crate::infrastructure::firestore_message_repository::FirestoreMessageRepository;
use crate::infrastructure::firestore_user_repository::FirestoreUserRepository;
use crate::infrastructure::gcs_object_storage::{GcsObjectStorage, GcsObjectStorageError};
use crate::infrastructure::iap_google_identity::GoogleIapIdentityProvider;
use crate::infrastructure::iap_test_identity::TestIapIdentityProvider;
use std::sync::Arc;

#[napi]
pub struct Service {
    authentication: IapAuthenticationService,
    file_upload: FileUploadService<GcsObjectStorage>,
    pub(crate) users: UserService,
    pub(crate) chats: ChatService,
    pub(crate) messages: MessageService,
    pub(crate) artifacts: ArtifactService,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Authentication(#[from] IapAuthenticationError),
    #[error(transparent)]
    Configuration(#[from] GcsObjectStorageError),
    #[error("native service configuration failed: {0}")]
    FirestoreConfiguration(String),
    #[error(transparent)]
    Upload(#[from] FileUploadError),
    #[error(transparent)]
    User(#[from] UserServiceError),
    #[error(transparent)]
    Chat(#[from] ChatServiceError),
    #[error(transparent)]
    Message(#[from] MessageServiceError),
    #[error(transparent)]
    Artifact(#[from] ArtifactServiceError),
    #[error("invalid native request: {0}")]
    InvalidRequest(String),
}

impl Service {
    pub fn new(
        authentication: IapAuthenticationService,
        file_upload: FileUploadService<GcsObjectStorage>,
        users: UserService,
        chats: ChatService,
        messages: MessageService,
        artifacts: ArtifactService,
    ) -> Self {
        Self {
            authentication,
            file_upload,
            users,
            chats,
            messages,
            artifacts,
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
    let project_id = required_environment("FIRESTORE_PROJECT_ID")?;
    let database_id = required_environment("FIRESTORE_DATABASE_ID")?;
    if database_id == "(default)" {
        return Err(ServiceError::FirestoreConfiguration(
            "FIRESTORE_DATABASE_ID must identify a named Firestore database".to_string(),
        ));
    }
    let db = crate::infrastructure::firestore::connect(&project_id, &database_id)
        .await
        .map_err(|error| ServiceError::FirestoreConfiguration(error.to_string()))?;

    let users: Arc<dyn UserRepository> = Arc::new(FirestoreUserRepository::new(db.clone()));
    let chats: Arc<dyn ChatRepository> = Arc::new(FirestoreChatRepository::new(db.clone()));
    let messages: Arc<dyn MessageRepository> =
        Arc::new(FirestoreMessageRepository::new(db.clone()));
    let artifacts: Arc<dyn ArtifactRepository> = Arc::new(FirestoreArtifactRepository::new(db));

    Ok(Service::new(
        authentication,
        FileUploadService::new(storage),
        UserService::new(users),
        ChatService::new(chats),
        MessageService::new(messages),
        ArtifactService::new(artifacts),
    ))
}

fn required_environment(name: &str) -> Result<String, ServiceError> {
    std::env::var(name)
        .map_err(|_| ServiceError::FirestoreConfiguration(format!("{name} must be configured")))
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn create_iap_provider() -> Result<Arc<dyn IapIdentityProvider>, IapIdentityProviderError> {
    let provider = std::env::var("IAP_AUTH_PROVIDER").unwrap_or_else(|_| "google".to_string());

    match provider.as_str() {
        "google" => Ok(Arc::new(GoogleIapIdentityProvider::from_env()?)),
        "test" => {
            // The local real-resource e2e runner uses a production Next.js
            // server so it does not need a file watcher. Its explicit marker
            // is never set by Cloud Run deployments.
            let real_e2e_test = std::env::var("E2E_REAL_TESTS").as_deref() == Ok("1");
            if is_production_environment() && !real_e2e_test {
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
