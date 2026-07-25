use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{error::ValidationError, iap_identity::IapIdentity, validate_identifier};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub iap_subject: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(
        id: impl AsRef<str>,
        email: impl Into<String>,
        iap_subject: Option<&str>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, ValidationError> {
        let email = email.into().trim().to_string();
        if email.is_empty() {
            return Err(ValidationError::Empty { field: "email" });
        }
        Ok(Self {
            id: validate_identifier(id.as_ref())?,
            email,
            iap_subject: iap_subject
                .map(super::validation::validate_subject)
                .transpose()?,
            created_at,
            updated_at,
        })
    }

    pub fn from_iap_identity(
        identity: &IapIdentity,
        now: DateTime<Utc>,
    ) -> Result<Self, ValidationError> {
        Self::new(
            identity.user_key(),
            identity.email.clone(),
            Some(identity.subject.as_str()),
            now,
            now,
        )
    }
}
