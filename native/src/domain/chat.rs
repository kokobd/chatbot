use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{error::ValidationError, pagination::PaginationPosition, validate_identifier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        match value {
            "public" => Ok(Self::Public),
            "private" => Ok(Self::Private),
            _ => Err(ValidationError::InvalidEnum {
                kind: "visibility",
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    Active,
    Archived,
    Deleted,
}

impl LifecycleState {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        match value {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "deleted" => Ok(Self::Deleted),
            _ => Err(ValidationError::InvalidEnum {
                kind: "lifecycle state",
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chat {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub visibility: Visibility,
    pub lifecycle: LifecycleState,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub lifecycle_revision: u64,
}

impl Chat {
    pub fn new(
        id: impl AsRef<str>,
        user_id: impl AsRef<str>,
        title: impl Into<String>,
        visibility: Visibility,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ValidationError> {
        let title = title.into().trim().to_string();
        if title.is_empty() {
            return Err(ValidationError::Empty { field: "title" });
        }
        Ok(Self {
            id: validate_identifier(id.as_ref())?,
            user_id: validate_identifier(user_id.as_ref())?,
            title,
            visibility,
            lifecycle: LifecycleState::Active,
            created_at,
            deleted_at: None,
            lifecycle_revision: 0,
        })
    }

    pub fn from_persisted(
        id: impl AsRef<str>,
        user_id: impl AsRef<str>,
        title: impl Into<String>,
        visibility: Visibility,
        lifecycle: LifecycleState,
        created_at: DateTime<Utc>,
        deleted_at: Option<DateTime<Utc>>,
        lifecycle_revision: u64,
    ) -> Result<Self, ValidationError> {
        if matches!(lifecycle, LifecycleState::Deleted) != deleted_at.is_some() {
            return Err(ValidationError::InvalidCharacters {
                field: "chat lifecycle tombstone",
            });
        }

        let mut chat = Self::new(id, user_id, title, visibility, created_at)?;
        chat.lifecycle = lifecycle;
        chat.deleted_at = deleted_at;
        chat.lifecycle_revision = lifecycle_revision;
        Ok(chat)
    }

    pub fn position(&self) -> PaginationPosition {
        PaginationPosition::new(self.created_at, self.id.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stream {
    pub id: String,
    pub chat_id: String,
    pub created_at: DateTime<Utc>,
}

impl Stream {
    pub fn new(
        id: impl AsRef<str>,
        chat_id: impl AsRef<str>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            id: validate_identifier(id.as_ref())?,
            chat_id: validate_identifier(chat_id.as_ref())?,
            created_at,
        })
    }
}
