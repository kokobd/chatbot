use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use crate::domain::{Chat, PaginationPosition, ValidationError, Visibility};

use super::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTitle(String);

impl ChatTitle {
    pub fn new(value: impl AsRef<str>) -> Result<Self, ValidationError> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(ValidationError::Empty { field: "title" });
        }
        if value.len() > crate::domain::validation::MAX_PAYLOAD_BYTES {
            return Err(ValidationError::TooLong {
                field: "title",
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
pub struct ChatHistoryCursor(PaginationPosition);

impl ChatHistoryCursor {
    pub fn new(position: PaginationPosition) -> Self {
        Self(position)
    }

    pub fn position(&self) -> &PaginationPosition {
        &self.0
    }

    /// Serializes both keyset values into an opaque transport cursor.
    pub fn encode(&self) -> Result<String, PersistenceError> {
        let payload = serde_json::to_vec(&self.0)
            .map_err(|error| PersistenceError::Serialization(error.to_string()))?;
        Ok(URL_SAFE_NO_PAD.encode(payload))
    }

    pub fn decode(value: impl AsRef<str>) -> Result<Self, PersistenceError> {
        let bytes = URL_SAFE_NO_PAD.decode(value.as_ref()).map_err(|error| {
            PersistenceError::InvalidInput(format!("invalid chat cursor: {error}"))
        })?;
        let position = serde_json::from_slice::<PaginationPosition>(&bytes).map_err(|error| {
            PersistenceError::InvalidInput(format!("invalid chat cursor payload: {error}"))
        })?;
        let position = PaginationPosition::try_new(position.created_at, position.id)
            .map_err(PersistenceError::from)?;
        Ok(Self(position))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatHistoryQuery {
    pub user_id: String,
    pub limit: u32,
    pub starting_after: Option<ChatHistoryCursor>,
    pub ending_before: Option<ChatHistoryCursor>,
}

impl ChatHistoryQuery {
    pub fn new(
        user_id: impl AsRef<str>,
        limit: u32,
        starting_after: Option<ChatHistoryCursor>,
        ending_before: Option<ChatHistoryCursor>,
    ) -> Result<Self, ValidationError> {
        if !(1..=50).contains(&limit) {
            return Err(ValidationError::InvalidCharacters {
                field: "history limit",
            });
        }
        if starting_after.is_some() && ending_before.is_some() {
            return Err(ValidationError::InvalidCharacters {
                field: "history cursors",
            });
        }
        Ok(Self {
            user_id: crate::domain::validate_identifier(user_id.as_ref())?,
            limit,
            starting_after,
            ending_before,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatHistoryPage {
    pub chats: Vec<Chat>,
    pub has_more: bool,
}

#[async_trait]
pub trait ChatRepository: Send + Sync {
    async fn find_chat(
        &self,
        user_id: &str,
        chat_id: &str,
    ) -> Result<Option<Chat>, PersistenceError>;

    async fn create_chat(&self, chat: &Chat) -> Result<Chat, PersistenceError>;

    async fn update_chat_title(
        &self,
        user_id: &str,
        chat_id: &str,
        title: &ChatTitle,
    ) -> Result<Chat, PersistenceError>;

    async fn update_chat_visibility(
        &self,
        user_id: &str,
        chat_id: &str,
        visibility: Visibility,
    ) -> Result<Chat, PersistenceError>;

    async fn delete_chat(&self, user_id: &str, chat_id: &str) -> Result<Chat, PersistenceError>;

    async fn list_chat_history(
        &self,
        query: &ChatHistoryQuery,
    ) -> Result<ChatHistoryPage, PersistenceError>;
}
