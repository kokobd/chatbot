use std::{pin::Pin, time::Duration};

use async_trait::async_trait;
use futures_core::Stream;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelContentPart {
    Text(String),
    Image { url: String, media_type: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMessage {
    pub role: ModelRole,
    pub parts: Vec<ModelContentPart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct ModelGenerationRequest {
    pub model_id: String,
    pub messages: Vec<ModelMessage>,
    pub request_id: String,
    pub timeout: Duration,
    pub cancellation: CancellationToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStreamEvent {
    ReasoningDelta(String),
    TextDelta(String),
    Usage(ModelUsage),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LanguageModelError {
    #[error("model provider authentication failed")]
    Authentication,
    #[error("model provider credits are insufficient")]
    Credits,
    #[error("model provider rate limit reached")]
    RateLimit,
    #[error("model provider is unavailable: {message}")]
    Unavailable { message: String, retryable: bool },
    #[error("model request is invalid: {message}")]
    InvalidRequest { message: String },
    #[error("model stream timed out")]
    Timeout,
    #[error("model stream was malformed: {message}")]
    MalformedStream { message: String },
    #[error("model request failed: {message}")]
    Unknown { message: String, retryable: bool },
}

impl LanguageModelError {
    pub fn retryable(&self) -> bool {
        match self {
            Self::RateLimit | Self::Timeout => true,
            Self::Unavailable { retryable, .. } | Self::Unknown { retryable, .. } => *retryable,
            Self::Authentication
            | Self::Credits
            | Self::InvalidRequest { .. }
            | Self::MalformedStream { .. } => false,
        }
    }
}

pub type ModelStream =
    Pin<Box<dyn Stream<Item = Result<ModelStreamEvent, LanguageModelError>> + Send>>;

#[async_trait]
pub trait LanguageModel: Send + Sync {
    async fn stream(
        &self,
        request: ModelGenerationRequest,
    ) -> Result<ModelStream, LanguageModelError>;
}
