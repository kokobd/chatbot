use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatStreamRequest {
    pub message: UserMessage,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserMessage {
    pub id: String,
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePart {
    Text {
        text: String,
    },
    Image {
        url: String,
        name: String,
        media_type: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditMessageRequest {
    pub message_id: String,
    pub replacement_parts: Vec<MessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatHistoryResponse {
    pub chats: Vec<ChatSummary>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub parts: Vec<MessagePart>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatResponse {
    pub id: String,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadResponse {
    pub pathname: String,
    pub url: String,
    pub filename: String,
    pub content_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub description: String,
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub vision: bool,
    pub reasoning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelsResponse {
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    BadRequest,
    Conflict,
    RateLimit,
    ProviderAuthentication,
    ProviderCredits,
    ProviderUnavailable,
    ProviderInvalidRequest,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum ChatStreamEvent {
    Status { phase: String, message: String },
    ReasoningDelta { delta: String },
    TextDelta { delta: String },
    Complete {
        message: ChatMessage,
        usage: Option<Usage>,
    },
    Error(ApiError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_parts_use_stable_wire_names() {
        let message = UserMessage {
            id: "message-1".to_string(),
            parts: vec![MessagePart::Image {
                url: "https://example.test/image.png".to_string(),
                name: "image.png".to_string(),
                media_type: "image/png".to_string(),
            }],
        };

        let value = serde_json::to_value(message).unwrap();
        assert_eq!(value["parts"][0]["type"], "image");
        assert_eq!(value["parts"][0]["media_type"], "image/png");
    }

    #[test]
    fn stream_events_round_trip() {
        let event = ChatStreamEvent::TextDelta {
            delta: "hello".to_string(),
        };
        let encoded = serde_json::to_string(&event).unwrap();
        let decoded: ChatStreamEvent = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, event);
    }
}
