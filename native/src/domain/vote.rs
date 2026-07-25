use serde::{Deserialize, Serialize};

use super::{error::ValidationError, validate_identifier};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vote {
    pub chat_id: String,
    pub message_id: String,
    pub is_upvoted: bool,
}

impl Vote {
    pub fn new(
        chat_id: impl AsRef<str>,
        message_id: impl AsRef<str>,
        is_upvoted: bool,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            chat_id: validate_identifier(chat_id.as_ref())?,
            message_id: validate_identifier(message_id.as_ref())?,
            is_upvoted,
        })
    }
}
