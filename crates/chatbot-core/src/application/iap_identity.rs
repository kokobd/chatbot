use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IapRequestEvidence {
    pub jwt_assertion: Option<String>,
    pub authenticated_user_email: Option<String>,
    pub authenticated_user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedIdentity {
    pub subject: String,
    pub email: String,
}

#[derive(Debug, Clone, Error)]
pub enum IapIdentityProviderError {
    #[error("request is not authenticated")]
    Unauthenticated,
    #[error("identity provider configuration error: {0}")]
    Configuration(String),
    #[error("identity provider is unavailable: {0}")]
    Unavailable(String),
    #[error("identity provider rejected the request: {0}")]
    Rejected(String),
}

#[async_trait]
pub trait IapIdentityProvider: Send + Sync {
    async fn authenticate(
        &self,
        evidence: &IapRequestEvidence,
    ) -> Result<AuthenticatedIdentity, IapIdentityProviderError>;
}
