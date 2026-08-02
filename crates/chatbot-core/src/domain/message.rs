use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    error::ValidationError,
    json::JsonValue,
    pagination::PaginationPosition,
    validation::{validate_identifier, validate_serialized_payload, MAX_PAYLOAD_BYTES},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

impl MessageRole {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        match value {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            "tool" => Ok(Self::Tool),
            _ => Err(ValidationError::InvalidEnum {
                kind: "message role",
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub chat_id: String,
    pub user_id: String,
    pub role: MessageRole,
    pub parts: JsonValue,
    pub attachments: JsonValue,
    pub created_at: DateTime<Utc>,
}

impl Message {
    pub fn new(
        id: impl AsRef<str>,
        chat_id: impl AsRef<str>,
        user_id: impl AsRef<str>,
        role: MessageRole,
        parts: JsonValue,
        attachments: JsonValue,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ValidationError> {
        validate_serialized_payload("message parts", &parts, MAX_PAYLOAD_BYTES)?;
        validate_serialized_payload("message attachments", &attachments, MAX_PAYLOAD_BYTES)?;
        Ok(Self {
            id: validate_identifier(id.as_ref())?,
            chat_id: validate_identifier(chat_id.as_ref())?,
            user_id: validate_identifier(user_id.as_ref())?,
            role,
            parts,
            attachments,
            created_at,
        })
    }

    pub fn position(&self) -> PaginationPosition {
        PaginationPosition::new(self.created_at, self.id.clone())
    }
}
