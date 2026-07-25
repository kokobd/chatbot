use async_trait::async_trait;

use crate::application::iap_identity::{
    AuthenticatedIdentity, IapIdentityProvider, IapIdentityProviderError, IapRequestEvidence,
};

pub struct TestIapIdentityProvider;

#[async_trait]
impl IapIdentityProvider for TestIapIdentityProvider {
    async fn authenticate(
        &self,
        evidence: &IapRequestEvidence,
    ) -> Result<AuthenticatedIdentity, IapIdentityProviderError> {
        let subject = evidence
            .authenticated_user_id
            .as_deref()
            .and_then(strip_accounts_namespace);
        let email = evidence
            .authenticated_user_email
            .as_deref()
            .and_then(strip_accounts_namespace);

        match (subject, email) {
            (Some(subject), Some(email))
                if !subject.trim().is_empty() && !email.trim().is_empty() =>
            {
                Ok(AuthenticatedIdentity {
                    email: email.to_string(),
                    subject: subject.to_string(),
                })
            }
            _ => Err(IapIdentityProviderError::Unauthenticated),
        }
    }
}

fn strip_accounts_namespace(value: &str) -> Option<&str> {
    value.strip_prefix("accounts.google.com:")
}

#[cfg(test)]
mod tests {
    use super::TestIapIdentityProvider;
    use crate::application::iap_identity::{IapIdentityProvider, IapRequestEvidence};

    #[tokio::test]
    async fn reads_namespaced_identity_headers() {
        let provider = TestIapIdentityProvider;
        let identity = provider
            .authenticate(&IapRequestEvidence {
                authenticated_user_email: Some("accounts.google.com:user@example.com".to_string()),
                authenticated_user_id: Some("accounts.google.com:subject-1".to_string()),
                jwt_assertion: None,
            })
            .await
            .unwrap();

        assert_eq!(identity.email, "user@example.com");
        assert_eq!(identity.subject, "subject-1");
    }

    #[tokio::test]
    async fn rejects_missing_identity_headers() {
        let provider = TestIapIdentityProvider;
        assert!(provider
            .authenticate(&IapRequestEvidence::default())
            .await
            .is_err());
    }
}
