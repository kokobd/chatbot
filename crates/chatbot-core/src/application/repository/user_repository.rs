use async_trait::async_trait;

use chrono::{DateTime, Utc};

use crate::domain::{iap_user_key, IapIdentity, IapSubject, User, ValidationError};

use super::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn new(value: impl AsRef<str>) -> Result<Self, ValidationError> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(ValidationError::Empty { field: "email" });
        }
        if value.len() > crate::domain::validation::MAX_PAYLOAD_BYTES {
            return Err(ValidationError::TooLong {
                field: "email",
                max_bytes: crate::domain::validation::MAX_PAYLOAD_BYTES,
            });
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IapUser {
    subject: IapSubject,
    user: User,
}

impl IapUser {
    pub fn from_identity(
        identity: &IapIdentity,
        now: DateTime<Utc>,
    ) -> Result<Self, ValidationError> {
        Self::new(
            identity.subject.clone(),
            User::from_iap_identity(identity, now)?,
        )
    }

    pub fn new(subject: IapSubject, user: User) -> Result<Self, ValidationError> {
        let expected_id = iap_user_key(subject.as_str())?;
        if user.id != expected_id || user.iap_subject.as_deref() != Some(subject.as_str()) {
            return Err(ValidationError::InvalidCharacters {
                field: "IAP user identity",
            });
        }
        Ok(Self { subject, user })
    }

    pub fn subject(&self) -> &IapSubject {
        &self.subject
    }

    pub fn user(&self) -> &User {
        &self.user
    }

    pub fn into_user(self) -> User {
        self.user
    }
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Looks up the user bound to `subject`.
    async fn find_iap_user(
        &self,
        subject: &IapSubject,
    ) -> Result<Option<IapUser>, PersistenceError>;

    /// Atomically inserts the user if its deterministic identity is absent.
    /// `Conflict` means another writer owns that identity.
    async fn create_iap_user(&self, user: &IapUser) -> Result<IapUser, PersistenceError>;

    /// Atomically changes only the email intent and returns the committed user.
    async fn update_iap_email(
        &self,
        subject: &IapSubject,
        email: &Email,
    ) -> Result<IapUser, PersistenceError>;
}
