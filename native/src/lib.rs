mod application;
pub mod domain;
mod infrastructure;
mod service;

pub use application::repository::{Email, IapUser, PersistenceError, UserRepository};
pub use application::user_service::{UserService, UserServiceError};
pub use infrastructure::firestore_user_repository::FirestoreUserRepository;

use application::file_upload::UploadResult as ApplicationUploadResult;
use application::iap_authentication::IapAuthenticationError;
use application::iap_identity::{
    AuthenticatedIdentity as ApplicationAuthenticatedIdentity, IapIdentityProviderError,
    IapRequestEvidence as ApplicationIapRequestEvidence,
};
use bytes::Bytes;
use napi::bindgen_prelude::{Buffer, External};
use napi::{Error, Result, Status};
use napi_derive::napi;
use service::{create_service as compose_service, Service, ServiceError};

#[napi(object)]
pub struct IapRequestHeaders {
    #[napi(js_name = "jwtAssertion")]
    pub jwt_assertion: Option<String>,
    #[napi(js_name = "authenticatedUserEmail")]
    pub authenticated_user_email: Option<String>,
    #[napi(js_name = "authenticatedUserId")]
    pub authenticated_user_id: Option<String>,
}

#[napi(object)]
pub struct AuthenticatedIdentity {
    pub subject: String,
    pub email: String,
}

#[napi(object)]
pub struct UploadResult {
    pub url: String,
    pub pathname: String,
    #[napi(js_name = "contentType")]
    pub content_type: String,
}

impl From<ApplicationUploadResult> for UploadResult {
    fn from(result: ApplicationUploadResult) -> Self {
        Self {
            url: result.url,
            pathname: result.pathname,
            content_type: result.content_type,
        }
    }
}

fn to_napi_error(error: ServiceError) -> Error {
    let status = match error {
        ServiceError::Authentication(IapAuthenticationError::Provider(
            IapIdentityProviderError::Configuration(_),
        )) => Status::InvalidArg,
        ServiceError::Authentication(_) => Status::GenericFailure,
        ServiceError::Configuration(_) => Status::InvalidArg,
        ServiceError::Upload(_) => Status::GenericFailure,
    };

    Error::new(status, error.to_string())
}

#[napi(js_name = "createService")]
pub async fn create_service() -> Result<External<Service>> {
    compose_service()
        .await
        .map(External::new)
        .map_err(to_napi_error)
}

impl From<ApplicationAuthenticatedIdentity> for AuthenticatedIdentity {
    fn from(identity: ApplicationAuthenticatedIdentity) -> Self {
        Self {
            email: identity.email,
            subject: identity.subject,
        }
    }
}

#[napi(js_name = "authenticateIapRequest")]
pub async fn authenticate_iap_request(
    service: &External<Service>,
    headers: IapRequestHeaders,
) -> Result<Option<AuthenticatedIdentity>> {
    let evidence = ApplicationIapRequestEvidence {
        authenticated_user_email: headers.authenticated_user_email,
        authenticated_user_id: headers.authenticated_user_id,
        jwt_assertion: headers.jwt_assertion,
    };

    service
        .authenticate_iap(&evidence)
        .await
        .map(|identity| identity.map(AuthenticatedIdentity::from))
        .map_err(to_napi_error)
}

#[napi(js_name = "uploadObject")]
pub async fn upload_object(
    service: &External<Service>,
    data: Buffer,
    filename: String,
    content_type: String,
) -> Result<UploadResult> {
    service
        .upload_object(Bytes::from(data.to_vec()), filename, content_type)
        .await
        .map(UploadResult::from)
        .map_err(to_napi_error)
}
