use std::time::Duration;

use async_trait::async_trait;
use chatbot_core::application::language_model::{
    LanguageModel, LanguageModelError, ModelContentPart, ModelGenerationRequest, ModelMessage,
    ModelRole, ModelStream, ModelStreamEvent, ModelUsage,
};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::time::timeout;

const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

#[derive(Clone)]
pub struct OpenRouter {
    client: Client,
    api_key: String,
    endpoint: String,
    http_referer: Option<String>,
    app_name: Option<String>,
    request_timeout: Duration,
    read_timeout: Duration,
}

impl OpenRouter {
    pub fn new(
        api_key: String,
        http_referer: Option<String>,
        app_name: Option<String>,
        request_timeout: Duration,
        read_timeout: Duration,
    ) -> Result<Self, LanguageModelError> {
        if api_key.trim().is_empty() {
            return Err(LanguageModelError::Authentication);
        }

        let client = Client::builder()
            .connect_timeout(request_timeout)
            .build()
            .map_err(|_| LanguageModelError::Unavailable {
                message: "OpenRouter client configuration failed".to_string(),
                retryable: false,
            })?;

        Ok(Self {
            client,
            api_key,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            http_referer,
            app_name,
            request_timeout,
            read_timeout,
        })
    }

    fn request_body(&self, request: &ModelGenerationRequest) -> OpenRouterRequest {
        OpenRouterRequest {
            model: request.model_id.clone(),
            messages: request.messages.iter().map(encode_message).collect(),
            stream: true,
            stream_options: Some(json!({"include_usage": true})),
        }
    }

    async fn send_once(
        &self,
        request: &ModelGenerationRequest,
    ) -> Result<reqwest::Response, LanguageModelError> {
        let mut builder = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&self.request_body(request));
        if let Some(value) = &self.http_referer {
            builder = builder.header("HTTP-Referer", value);
        }
        if let Some(value) = &self.app_name {
            builder = builder.header("X-Title", value);
        }

        let response = timeout(self.request_timeout, builder.send())
            .await
            .map_err(|_| LanguageModelError::Timeout)?
            .map_err(|_| LanguageModelError::Unavailable {
                message: "OpenRouter request failed".to_string(),
                retryable: true,
            })?;

        if response.status().is_success() {
            return Ok(response);
        }

        Err(classify_http_error(response.status()))
    }
}

#[async_trait]
impl LanguageModel for OpenRouter {
    async fn stream(
        &self,
        request: ModelGenerationRequest,
    ) -> Result<ModelStream, LanguageModelError> {
        let response = match self.send_once(&request).await {
            Ok(response) => response,
            Err(first) if first.retryable() => self.send_once(&request).await.map_err(|_| first)?,
            Err(error) => return Err(error),
        };

        let bytes = response.bytes_stream();
        let cancellation = request.cancellation.clone();
        let read_timeout = self.read_timeout;
        let stream = async_stream::stream! {
            let mut bytes = Box::pin(bytes);
            let mut pending = String::new();

            loop {
                let item = tokio::select! {
                    _ = cancellation.cancelled() => None,
                    item = timeout(read_timeout, bytes.next()) => Some(item),
                };
                let Some(item) = item else { break };
                let chunk = match item {
                    Err(_) => {
                        yield Err(LanguageModelError::Timeout);
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(Err(_))) => {
                        yield Err(LanguageModelError::Unavailable {
                            message: "OpenRouter stream read failed".to_string(),
                            retryable: true,
                        });
                        break;
                    }
                    Ok(Some(Ok(chunk))) => chunk,
                };
                pending.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(newline) = pending.find('\n') {
                    let line = pending.drain(..=newline).collect::<String>();
                    let line = line.trim();
                    if let Some(data) = line.strip_prefix("data:") {
                        let data = data.trim();
                        if data == "[DONE]" {
                            break;
                        }
                        if data.is_empty() {
                            continue;
                        }
                        match decode_event(data) {
                            Ok(events) => {
                                for event in events {
                                    yield Ok(event);
                                }
                            }
                            Err(error) => {
                                yield Err(error);
                                break;
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<Value>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<Value>,
}

fn encode_message(message: &ModelMessage) -> Value {
    let role = match message.role {
        ModelRole::System => "system",
        ModelRole::User => "user",
        ModelRole::Assistant => "assistant",
    };
    let parts: Vec<Value> = message
        .parts
        .iter()
        .map(|part| match part {
            ModelContentPart::Text(text) => json!({"type": "text", "text": text}),
            ModelContentPart::Image { url, media_type } => json!({
                "type": "image_url",
                "image_url": {"url": url, "media_type": media_type}
            }),
        })
        .collect();
    json!({"role": role, "content": parts})
}

fn decode_event(data: &str) -> Result<Vec<ModelStreamEvent>, LanguageModelError> {
    let value: Value =
        serde_json::from_str(data).map_err(|_| LanguageModelError::MalformedStream {
            message: "OpenRouter returned malformed stream data".to_string(),
        })?;
    if let Some(error) = value.get("error") {
        return Err(LanguageModelError::Unavailable {
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("OpenRouter stream error")
                .to_string(),
            retryable: false,
        });
    }

    let mut events = Vec::new();
    if let Some(choice) = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
    {
        let delta = choice.get("delta").unwrap_or(&Value::Null);
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                events.push(ModelStreamEvent::TextDelta(text.to_string()));
            }
        }
        if let Some(reasoning) = delta
            .get("reasoning")
            .or_else(|| delta.get("reasoning_content"))
            .and_then(Value::as_str)
        {
            if !reasoning.is_empty() {
                events.push(ModelStreamEvent::ReasoningDelta(reasoning.to_string()));
            }
        }
    }
    if let Some(usage) = value.get("usage") {
        events.push(ModelStreamEvent::Usage(ModelUsage {
            prompt_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
            completion_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
            total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
        }));
    }
    Ok(events)
}

fn classify_http_error(status: StatusCode) -> LanguageModelError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => LanguageModelError::Authentication,
        StatusCode::PAYMENT_REQUIRED => LanguageModelError::Credits,
        StatusCode::TOO_MANY_REQUESTS => LanguageModelError::RateLimit,
        status if status.is_server_error() => LanguageModelError::Unavailable {
            message: "OpenRouter is unavailable".to_string(),
            retryable: true,
        },
        _ => LanguageModelError::InvalidRequest {
            message: "OpenRouter rejected the request".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_http_error, decode_event, encode_message};
    use chatbot_core::application::language_model::{ModelContentPart, ModelMessage, ModelRole};
    use reqwest::StatusCode;

    #[test]
    fn encodes_multimodal_messages_without_provider_types_in_core() {
        let value = encode_message(&ModelMessage {
            role: ModelRole::User,
            parts: vec![
                ModelContentPart::Text("hello".into()),
                ModelContentPart::Image {
                    url: "https://x/y".into(),
                    media_type: "image/png".into(),
                },
            ],
        });
        assert_eq!(value["content"][1]["type"], "image_url");
    }

    #[test]
    fn decodes_text_reasoning_and_usage() {
        let events = decode_event(r#"{"choices":[{"delta":{"content":"hi","reasoning":"why"}}],"usage":{"total_tokens":3}}"#).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn maps_provider_statuses_without_leaking_response_bodies() {
        assert!(classify_http_error(StatusCode::TOO_MANY_REQUESTS).retryable());
        assert!(!classify_http_error(StatusCode::BAD_REQUEST).retryable());
    }
}
