use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::application::iap_identity::{
    AuthenticatedIdentity, IapIdentityProvider, IapIdentityProviderError, IapRequestEvidence,
};

const DEFAULT_ISSUER: &str = "https://cloud.google.com/iap";
const DEFAULT_PUBLIC_KEY_URL: &str = "https://www.gstatic.com/iap/verify/public_key";
const CLOCK_SKEW_SECONDS: u64 = 30;
const KEY_CACHE_TTL_SECONDS: u64 = 300;

#[derive(Clone)]
pub struct GoogleIapIdentityProvider {
    audience: String,
    issuer: String,
    key_url: String,
    client: Client,
    keys: Arc<RwLock<Option<CachedKeys>>>,
}

struct CachedKeys {
    expires_at: SystemTime,
    keys: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct IapClaims {
    aud: String,
    email: String,
    exp: u64,
    iat: u64,
    iss: String,
    sub: String,
}

impl GoogleIapIdentityProvider {
    pub fn from_env() -> Result<Self, IapIdentityProviderError> {
        let audience = std::env::var("IAP_JWT_AUDIENCE").map_err(|_| {
            IapIdentityProviderError::Configuration("IAP_JWT_AUDIENCE is required".to_string())
        })?;
        if audience.trim().is_empty() {
            return Err(IapIdentityProviderError::Configuration(
                "IAP_JWT_AUDIENCE must not be empty".to_string(),
            ));
        }

        Ok(Self {
            audience,
            issuer: std::env::var("IAP_JWT_ISSUER").unwrap_or_else(|_| DEFAULT_ISSUER.to_string()),
            key_url: std::env::var("IAP_JWT_PUBLIC_KEYS_URL")
                .unwrap_or_else(|_| DEFAULT_PUBLIC_KEY_URL.to_string()),
            client: Client::new(),
            keys: Arc::new(RwLock::new(None)),
        })
    }

    async fn public_key(&self, key_id: &str) -> Result<String, IapIdentityProviderError> {
        if let Some(keys) = self.cached_keys().await {
            if let Some(key) = keys.get(key_id) {
                return Ok(key.clone());
            }
        }

        self.refresh_keys().await?;
        self.cached_keys()
            .await
            .and_then(|keys| keys.get(key_id).cloned())
            .ok_or_else(|| {
                IapIdentityProviderError::Rejected("unknown IAP signing key".to_string())
            })
    }

    async fn cached_keys(&self) -> Option<HashMap<String, String>> {
        let guard = self.keys.read().await;
        let cached = guard.as_ref()?;
        if cached.expires_at > SystemTime::now() {
            Some(cached.keys.clone())
        } else {
            None
        }
    }

    async fn refresh_keys(&self) -> Result<(), IapIdentityProviderError> {
        let keys = self
            .client
            .get(&self.key_url)
            .send()
            .await
            .map_err(|error| IapIdentityProviderError::Unavailable(error.to_string()))?
            .error_for_status()
            .map_err(|error| IapIdentityProviderError::Unavailable(error.to_string()))?
            .json::<HashMap<String, String>>()
            .await
            .map_err(|error| IapIdentityProviderError::Unavailable(error.to_string()))?;

        let mut guard = self.keys.write().await;
        *guard = Some(CachedKeys {
            expires_at: SystemTime::now() + Duration::from_secs(KEY_CACHE_TTL_SECONDS),
            keys,
        });
        Ok(())
    }
}

#[async_trait]
impl IapIdentityProvider for GoogleIapIdentityProvider {
    async fn authenticate(
        &self,
        evidence: &IapRequestEvidence,
    ) -> Result<AuthenticatedIdentity, IapIdentityProviderError> {
        let assertion = evidence
            .jwt_assertion
            .as_deref()
            .ok_or(IapIdentityProviderError::Unauthenticated)?;
        let header = decode_header(assertion)
            .map_err(|_| IapIdentityProviderError::Rejected("malformed IAP JWT".to_string()))?;

        if header.alg != Algorithm::ES256 {
            return Err(IapIdentityProviderError::Rejected(
                "IAP JWT must use ES256".to_string(),
            ));
        }

        let key_id = header.kid.ok_or_else(|| {
            IapIdentityProviderError::Rejected("IAP JWT has no key ID".to_string())
        })?;
        let public_key = self.public_key(&key_id).await?;
        let decoding_key = DecodingKey::from_ec_pem(public_key.as_bytes()).map_err(|_| {
            IapIdentityProviderError::Rejected("invalid IAP public key".to_string())
        })?;

        let mut validation = Validation::new(Algorithm::ES256);
        validation.leeway = CLOCK_SKEW_SECONDS;
        validation.set_issuer(std::slice::from_ref(&self.issuer));
        validation.set_audience(std::slice::from_ref(&self.audience));

        let token = decode::<IapClaims>(assertion, &decoding_key, &validation)
            .map_err(|_| IapIdentityProviderError::Rejected("invalid IAP JWT".to_string()))?;
        validate_audience_and_issuer(&token.claims, &self.audience, &self.issuer)?;
        validate_issue_time(&token.claims)?;
        validate_identity_headers(&token.claims, evidence)?;

        Ok(AuthenticatedIdentity {
            email: token.claims.email,
            subject: token.claims.sub,
        })
    }
}

fn validate_audience_and_issuer(
    claims: &IapClaims,
    expected_audience: &str,
    expected_issuer: &str,
) -> Result<(), IapIdentityProviderError> {
    if claims.aud != expected_audience || claims.iss != expected_issuer {
        return Err(IapIdentityProviderError::Rejected(
            "IAP JWT audience or issuer does not match configuration".to_string(),
        ));
    }

    Ok(())
}

fn validate_issue_time(claims: &IapClaims) -> Result<(), IapIdentityProviderError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| IapIdentityProviderError::Unavailable("system clock is invalid".to_string()))?
        .as_secs();

    if claims.iat > now.saturating_add(CLOCK_SKEW_SECONDS)
        || claims.exp.saturating_add(CLOCK_SKEW_SECONDS) < now
        || claims.exp < claims.iat
    {
        return Err(IapIdentityProviderError::Rejected(
            "IAP JWT has invalid issue or expiry time".to_string(),
        ));
    }

    Ok(())
}

fn validate_identity_headers(
    claims: &IapClaims,
    evidence: &IapRequestEvidence,
) -> Result<(), IapIdentityProviderError> {
    if let Some(email) = evidence.authenticated_user_email.as_deref() {
        if strip_accounts_namespace(email) != Some(claims.email.as_str()) {
            return Err(IapIdentityProviderError::Rejected(
                "IAP email header does not match JWT".to_string(),
            ));
        }
    }

    if let Some(subject) = evidence.authenticated_user_id.as_deref() {
        if subject != claims.sub {
            return Err(IapIdentityProviderError::Rejected(
                "IAP user ID header does not match JWT".to_string(),
            ));
        }
    }

    Ok(())
}

fn strip_accounts_namespace(value: &str) -> Option<&str> {
    value.strip_prefix("accounts.google.com:")
}

#[cfg(test)]
mod tests {
    use super::{
        validate_audience_and_issuer, validate_identity_headers, validate_issue_time, IapClaims,
    };
    use crate::application::iap_identity::IapRequestEvidence;

    fn claims() -> IapClaims {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        IapClaims {
            aud: "expected-audience".to_string(),
            email: "user@example.com".to_string(),
            exp: now + 60,
            iat: now,
            iss: "https://cloud.google.com/iap".to_string(),
            sub: "accounts.google.com:subject-1".to_string(),
        }
    }

    #[test]
    fn accepts_matching_issuer_and_audience() {
        assert!(validate_audience_and_issuer(
            &claims(),
            "expected-audience",
            "https://cloud.google.com/iap"
        )
        .is_ok());
    }

    #[test]
    fn rejects_mismatched_identity_headers() {
        let evidence = IapRequestEvidence {
            authenticated_user_email: Some("accounts.google.com:other@example.com".to_string()),
            authenticated_user_id: Some("accounts.google.com:subject-1".to_string()),
            jwt_assertion: None,
        };

        assert!(validate_identity_headers(&claims(), &evidence).is_err());
    }

    #[test]
    fn accepts_matching_namespaced_user_id_header() {
        let evidence = IapRequestEvidence {
            authenticated_user_email: Some("accounts.google.com:user@example.com".to_string()),
            authenticated_user_id: Some("accounts.google.com:subject-1".to_string()),
            jwt_assertion: None,
        };

        assert!(validate_identity_headers(&claims(), &evidence).is_ok());
    }

    #[test]
    fn rejects_future_issue_times() {
        let mut claims = claims();
        claims.iat += 60;
        assert!(validate_issue_time(&claims).is_err());
    }
}
