use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::domain::{JsonValue, Message};

use super::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageParts(JsonValue);

impl MessageParts {
    pub fn new(value: JsonValue) -> Result<Self, PersistenceError> {
        crate::domain::validation::validate_serialized_payload(
            "message parts",
            &value,
            crate::domain::validation::MAX_PAYLOAD_BYTES,
        )
        .map_err(PersistenceError::from)?;
        Ok(Self(value))
    }

    pub fn value(&self) -> &JsonValue {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageQuery {
    pub user_id: String,
    pub chat_id: String,
}

impl MessageQuery {
    pub fn new(
        user_id: impl AsRef<str>,
        chat_id: impl AsRef<str>,
    ) -> Result<Self, PersistenceError> {
        Ok(Self {
            user_id: crate::domain::validate_identifier(user_id.as_ref())?,
            chat_id: crate::domain::validate_identifier(chat_id.as_ref())?,
        })
    }
}

#[async_trait]
pub trait MessageRepository: Send + Sync {
    async fn save_messages(&self, messages: &[Message]) -> Result<Vec<Message>, PersistenceError>;

    async fn update_message(&self, message: &Message) -> Result<Message, PersistenceError>;

    async fn get_message_by_id(
        &self,
        user_id: &str,
        chat_id: &str,
        message_id: &str,
    ) -> Result<Option<Message>, PersistenceError>;

    async fn get_messages_by_chat_id(
        &self,
        query: &MessageQuery,
    ) -> Result<Vec<Message>, PersistenceError>;

    async fn count_user_messages(
        &self,
        user_id: &str,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, PersistenceError>;

    async fn delete_messages_after(
        &self,
        _user_id: &str,
        _chat_id: &str,
        _timestamp: DateTime<Utc>,
    ) -> Result<Vec<Message>, PersistenceError> {
        Err(PersistenceError::Internal {
            message: "message deletion is not configured".to_string(),
            retryable: false,
        })
    }
}
