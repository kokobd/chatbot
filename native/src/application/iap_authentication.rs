use std::sync::Arc;

use thiserror::Error;

use super::iap_identity::{
    AuthenticatedIdentity, IapIdentityProvider, IapIdentityProviderError, IapRequestEvidence,
};

pub struct IapAuthenticationService {
    provider: Arc<dyn IapIdentityProvider>,
}

#[derive(Debug, Error)]
pub enum IapAuthenticationError {
    #[error(transparent)]
    Provider(#[from] IapIdentityProviderError),
    #[error("authenticated identity has an empty subject")]
    EmptySubject,
    #[error("authenticated identity has an empty email")]
    EmptyEmail,
}

impl IapAuthenticationService {
    pub fn new(provider: Arc<dyn IapIdentityProvider>) -> Self {
        Self { provider }
    }

    pub async fn authenticate(
        &self,
        evidence: &IapRequestEvidence,
    ) -> Result<Option<AuthenticatedIdentity>, IapAuthenticationError> {
        match self.provider.authenticate(evidence).await {
            Ok(identity) => {
                let identity = AuthenticatedIdentity {
                    email: identity.email.trim().to_string(),
                    subject: identity.subject.trim().to_string(),
                };

                if identity.subject.is_empty() {
                    return Err(IapAuthenticationError::EmptySubject);
                }

                if identity.email.is_empty() {
                    return Err(IapAuthenticationError::EmptyEmail);
                }

                Ok(Some(identity))
            }
            Err(IapIdentityProviderError::Unauthenticated) => Ok(None),
            Err(error) => Err(IapAuthenticationError::Provider(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IapAuthenticationService;
    use crate::application::iap_identity::{
        AuthenticatedIdentity, IapIdentityProvider, IapIdentityProviderError, IapRequestEvidence,
    };
    use async_trait::async_trait;
    use std::sync::Arc;

    struct FakeProvider {
        result: Result<AuthenticatedIdentity, IapIdentityProviderError>,
    }

    #[async_trait]
    impl IapIdentityProvider for FakeProvider {
        async fn authenticate(
            &self,
            _evidence: &IapRequestEvidence,
        ) -> Result<AuthenticatedIdentity, IapIdentityProviderError> {
            self.result.clone()
        }
    }

    #[tokio::test]
    async fn normalizes_a_provider_identity() {
        let service = IapAuthenticationService::new(Arc::new(FakeProvider {
            result: Ok(AuthenticatedIdentity {
                email: "  user@example.com ".to_string(),
                subject: " subject-1 ".to_string(),
            }),
        }));

        let identity = service
            .authenticate(&IapRequestEvidence::default())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(identity.email, "user@example.com");
        assert_eq!(identity.subject, "subject-1");
    }

    #[tokio::test]
    async fn maps_missing_credentials_to_no_identity() {
        let service = IapAuthenticationService::new(Arc::new(FakeProvider {
            result: Err(IapIdentityProviderError::Unauthenticated),
        }));

        assert!(service
            .authenticate(&IapRequestEvidence::default())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn preserves_provider_operational_errors() {
        let service = IapAuthenticationService::new(Arc::new(FakeProvider {
            result: Err(IapIdentityProviderError::Unavailable(
                "key server".to_string(),
            )),
        }));

        assert!(service
            .authenticate(&IapRequestEvidence::default())
            .await
            .is_err());
    }
}
