mod application;
pub mod domain;
mod infrastructure;
mod service;

pub use application::artifact_service::{ArtifactService, ArtifactServiceError};
pub use application::chat_service::{ChatService, ChatServiceError};
pub use application::message_service::{MessageService, MessageServiceError};
pub use application::repository::{
    ArtifactRepository, ChatHistoryCursor, ChatHistoryPage, ChatHistoryQuery, ChatRepository,
    ChatTitle, Email, IapUser, MessageParts, MessageQuery, MessageRepository, PersistenceError,
    UserRepository,
};
pub use application::user_service::{UserService, UserServiceError};
pub use infrastructure::firestore_artifact_repository::FirestoreArtifactRepository;
pub use infrastructure::firestore_chat_repository::FirestoreChatRepository;
pub use infrastructure::firestore_message_repository::FirestoreMessageRepository;
pub use infrastructure::firestore_user_repository::FirestoreUserRepository;

use application::artifact_service::ArtifactServiceError as NativeArtifactServiceError;
use application::file_upload::UploadResult as ApplicationUploadResult;
use application::iap_authentication::IapAuthenticationError;
use application::iap_identity::{
    AuthenticatedIdentity as ApplicationAuthenticatedIdentity, IapIdentityProviderError,
    IapRequestEvidence as ApplicationIapRequestEvidence,
};
use application::repository::chat_repository::ChatHistoryQuery as NativeChatHistoryQuery;
use application::repository::error::PersistenceError as NativePersistenceError;
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, Utc};
use napi::bindgen_prelude::{Buffer, External};
use napi::{Error, Result, Status};
use napi_derive::napi;
use serde::Serialize;
use serde_json::Value;
use service::{create_service as compose_service, Service, ServiceError};
use std::result::Result as StdResult;
use uuid::Uuid;

#[napi(object)]
pub struct IapRequestHeaders {
    #[napi(js_name = "jwtAssertion")]
    pub jwt_assertion: Option<String>,
    #[napi(js_name = "authenticatedUserEmail")]
    pub authenticated_user_email: Option<String>,
    #[napi(js_name = "authenticatedUserId")]
    pub authenticated_user_id: Option<String>,
}

#[napi(object)]
pub struct AuthenticatedIdentity {
    pub subject: String,
    pub email: String,
}

#[napi(object)]
pub struct UploadResult {
    pub url: String,
    pub pathname: String,
    #[napi(js_name = "contentType")]
    pub content_type: String,
}

#[napi(object)]
pub struct SecretEntryDto {
    pub key: String,
    pub value: String,
}

#[napi(object)]
pub struct UserDto {
    pub id: String,
    pub email: String,
    #[napi(js_name = "iapSubject")]
    pub iap_subject: Option<String>,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
    #[napi(js_name = "updatedAt")]
    pub updated_at: String,
}

#[napi(object)]
pub struct ChatDto {
    pub id: String,
    #[napi(js_name = "userId")]
    pub user_id: String,
    pub title: String,
    pub visibility: String,
    pub lifecycle: String,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
    #[napi(js_name = "deletedAt")]
    pub deleted_at: Option<String>,
    #[napi(js_name = "lifecycleRevision")]
    pub lifecycle_revision: i64,
}

#[napi(object)]
pub struct MessageDto {
    pub id: String,
    #[napi(js_name = "chatId")]
    pub chat_id: String,
    #[napi(js_name = "userId")]
    pub user_id: String,
    pub role: String,
    pub parts: String,
    pub attachments: String,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
}

#[napi(object)]
pub struct ArtifactDto {
    pub id: String,
    #[napi(js_name = "userId")]
    pub user_id: String,
    pub title: String,
    pub kind: String,
    pub content: Option<String>,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
    #[napi(js_name = "headVersionId")]
    pub head_version_id: Option<String>,
}

#[napi(object)]
pub struct DocumentDto {
    pub id: String,
    #[napi(js_name = "versionId")]
    pub version_id: String,
    #[napi(js_name = "userId")]
    pub user_id: String,
    pub title: String,
    pub kind: String,
    pub content: Option<String>,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
}

#[napi(object)]
pub struct SuggestionDto {
    pub id: String,
    #[napi(js_name = "documentId")]
    pub document_id: String,
    #[napi(js_name = "versionId")]
    pub version_id: String,
    #[napi(js_name = "userId")]
    pub user_id: String,
    #[napi(js_name = "originalText")]
    pub original_text: String,
    #[napi(js_name = "suggestedText")]
    pub suggested_text: String,
    pub description: Option<String>,
    #[napi(js_name = "isResolved")]
    pub is_resolved: bool,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
}

#[napi(object)]
pub struct VoteDto {
    #[napi(js_name = "chatId")]
    pub chat_id: String,
    #[napi(js_name = "messageId")]
    pub message_id: String,
    #[napi(js_name = "isUpvoted")]
    pub is_upvoted: bool,
}

#[napi(object)]
pub struct StreamDto {
    pub id: String,
    #[napi(js_name = "chatId")]
    pub chat_id: String,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
}

#[napi(object)]
pub struct MessageInput {
    pub id: String,
    #[napi(js_name = "chatId")]
    pub chat_id: String,
    #[napi(js_name = "userId")]
    pub user_id: String,
    pub role: String,
    pub parts: String,
    pub attachments: String,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
}

#[napi(object)]
pub struct SuggestionInput {
    pub id: String,
    #[napi(js_name = "documentId")]
    pub document_id: String,
    #[napi(js_name = "versionId")]
    pub version_id: String,
    #[napi(js_name = "userId")]
    pub user_id: String,
    #[napi(js_name = "originalText")]
    pub original_text: String,
    #[napi(js_name = "suggestedText")]
    pub suggested_text: String,
    pub description: Option<String>,
    #[napi(js_name = "isResolved")]
    pub is_resolved: bool,
    #[napi(js_name = "createdAt")]
    pub created_at: String,
}

#[napi(object)]
pub struct ChatHistoryDto {
    pub chats: Vec<ChatDto>,
    #[napi(js_name = "hasMore")]
    pub has_more: bool,
    #[napi(js_name = "nextCursor")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
struct NapiErrorPayload {
    category: String,
    retryable: bool,
    message: String,
    reconciliation: Option<Box<NapiErrorPayload>>,
}

fn persistence_error_payload(error: &NativePersistenceError) -> NapiErrorPayload {
    let (category, retryable, message, reconciliation) = match error {
        NativePersistenceError::NotFound => ("not_found", false, error.to_string(), None),
        NativePersistenceError::Conflict => ("conflict", false, error.to_string(), None),
        NativePersistenceError::OutcomeUnknown {
            message,
            retryable,
            reconciliation,
        } => (
            "outcome_unknown",
            *retryable,
            message.clone(),
            reconciliation
                .as_deref()
                .map(persistence_error_payload)
                .map(Box::new),
        ),
        NativePersistenceError::Unavailable { message, retryable } => {
            ("unavailable", *retryable, message.clone(), None)
        }
        NativePersistenceError::FailedPrecondition(message) => {
            ("failed_precondition", false, message.clone(), None)
        }
        NativePersistenceError::PermissionDenied(message) => {
            ("permission_denied", false, message.clone(), None)
        }
        NativePersistenceError::InvalidInput(message) => {
            ("invalid_input", false, message.clone(), None)
        }
        NativePersistenceError::CorruptData(message) => {
            ("corrupt_data", false, message.clone(), None)
        }
        NativePersistenceError::Serialization(message) => {
            ("serialization", false, message.clone(), None)
        }
        NativePersistenceError::Internal { message, retryable } => {
            ("internal", *retryable, message.clone(), None)
        }
    };
    NapiErrorPayload {
        category: category.to_string(),
        retryable,
        message,
        reconciliation,
    }
}

fn service_error_payload(error: &ServiceError) -> NapiErrorPayload {
    match error {
        ServiceError::User(error) => match error {
            crate::application::user_service::UserServiceError::Persistence(error) => {
                persistence_error_payload(error)
            }
            crate::application::user_service::UserServiceError::Validation(error) => {
                NapiErrorPayload {
                    category: "invalid_input".to_string(),
                    retryable: false,
                    message: error.to_string(),
                    reconciliation: None,
                }
            }
        },
        ServiceError::Chat(error) => match error {
            crate::application::chat_service::ChatServiceError::Persistence(error) => {
                persistence_error_payload(error)
            }
            crate::application::chat_service::ChatServiceError::Validation(error) => {
                NapiErrorPayload {
                    category: "invalid_input".to_string(),
                    retryable: false,
                    message: error.to_string(),
                    reconciliation: None,
                }
            }
        },
        ServiceError::Message(error) => match error {
            crate::application::message_service::MessageServiceError::Persistence(error) => {
                persistence_error_payload(error)
            }
            crate::application::message_service::MessageServiceError::Validation(error) => {
                NapiErrorPayload {
                    category: "invalid_input".to_string(),
                    retryable: false,
                    message: error.to_string(),
                    reconciliation: None,
                }
            }
        },
        ServiceError::Artifact(error) => match error {
            NativeArtifactServiceError::Persistence(error) => persistence_error_payload(error),
            NativeArtifactServiceError::Validation(error) => NapiErrorPayload {
                category: "invalid_input".to_string(),
                retryable: false,
                message: error.to_string(),
                reconciliation: None,
            },
        },
        ServiceError::InvalidRequest(message) => NapiErrorPayload {
            category: "invalid_input".to_string(),
            retryable: false,
            message: message.clone(),
            reconciliation: None,
        },
        _ => NapiErrorPayload {
            category: "internal".to_string(),
            retryable: false,
            message: error.to_string(),
            reconciliation: None,
        },
    }
}

fn to_napi_error<E>(error: E) -> Error
where
    E: Into<ServiceError>,
{
    let error: ServiceError = error.into();
    let status = match &error {
        ServiceError::Authentication(IapAuthenticationError::Provider(
            IapIdentityProviderError::Configuration(_),
        ))
        | ServiceError::FirestoreConfiguration(_)
        | ServiceError::SecretsConfiguration(_)
        | ServiceError::InvalidRequest(_) => Status::InvalidArg,
        ServiceError::Authentication(_) => Status::GenericFailure,
        _ => match service_error_payload(&error).category.as_str() {
            "invalid_input" => Status::InvalidArg,
            "not_found" | "permission_denied" => Status::GenericFailure,
            _ => Status::GenericFailure,
        },
    };
    let payload = serde_json::to_string(&service_error_payload(&error))
        .unwrap_or_else(|_| format!("{{\"category\":\"internal\",\"message\":{error:?}}}"));
    Error::new(status, payload)
}

fn boundary_error(message: impl Into<String>) -> ServiceError {
    ServiceError::InvalidRequest(message.into())
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn parse_timestamp(value: &str) -> StdResult<DateTime<Utc>, ServiceError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| boundary_error(format!("invalid timestamp: {error}")))
}

fn json_string(value: &Value) -> StdResult<String, ServiceError> {
    serde_json::to_string(value).map_err(|error| boundary_error(format!("invalid JSON: {error}")))
}

fn parse_json(value: &str) -> StdResult<Value, ServiceError> {
    serde_json::from_str(value).map_err(|error| boundary_error(format!("invalid JSON: {error}")))
}

fn content_text(value: &Option<Value>) -> StdResult<Option<String>, ServiceError> {
    value
        .as_ref()
        .map(|value| match value {
            Value::String(value) => Ok(value.clone()),
            value => json_string(value),
        })
        .transpose()
}

impl From<ApplicationUploadResult> for UploadResult {
    fn from(result: ApplicationUploadResult) -> Self {
        Self {
            url: result.url,
            pathname: result.pathname,
            content_type: result.content_type,
        }
    }
}

impl From<ApplicationAuthenticatedIdentity> for AuthenticatedIdentity {
    fn from(identity: ApplicationAuthenticatedIdentity) -> Self {
        Self {
            email: identity.email,
            subject: identity.subject,
        }
    }
}

fn user_dto(user: domain::User) -> UserDto {
    UserDto {
        id: user.id,
        email: user.email,
        iap_subject: user.iap_subject,
        created_at: timestamp(user.created_at),
        updated_at: timestamp(user.updated_at),
    }
}

fn chat_dto(chat: domain::Chat) -> ChatDto {
    ChatDto {
        id: chat.id,
        user_id: chat.user_id,
        title: chat.title,
        visibility: serde_json::to_string(&chat.visibility)
            .unwrap()
            .trim_matches('"')
            .to_string(),
        lifecycle: serde_json::to_string(&chat.lifecycle)
            .unwrap()
            .trim_matches('"')
            .to_string(),
        created_at: timestamp(chat.created_at),
        deleted_at: chat.deleted_at.map(timestamp),
        lifecycle_revision: chat.lifecycle_revision as i64,
    }
}

fn message_dto(message: domain::Message) -> StdResult<MessageDto, ServiceError> {
    Ok(MessageDto {
        id: message.id,
        chat_id: message.chat_id,
        user_id: message.user_id,
        role: serde_json::to_string(&message.role)
            .unwrap()
            .trim_matches('"')
            .to_string(),
        parts: json_string(&message.parts)?,
        attachments: json_string(&message.attachments)?,
        created_at: timestamp(message.created_at),
    })
}

fn document_dto(
    artifact: &domain::Artifact,
    version: domain::DocumentVersion,
) -> StdResult<DocumentDto, ServiceError> {
    Ok(DocumentDto {
        id: artifact.id.clone(),
        version_id: version.version_id,
        user_id: artifact.user_id.clone(),
        title: artifact.title.clone(),
        kind: serde_json::to_string(&artifact.kind)
            .unwrap()
            .trim_matches('"')
            .to_string(),
        content: content_text(&version.content)?,
        created_at: timestamp(version.created_at),
    })
}

fn suggestion_dto(suggestion: domain::Suggestion) -> SuggestionDto {
    SuggestionDto {
        id: suggestion.id,
        document_id: suggestion.document_id,
        version_id: suggestion.version_id,
        user_id: suggestion.user_id,
        original_text: suggestion.original_text,
        suggested_text: suggestion.suggested_text,
        description: suggestion.description,
        is_resolved: suggestion.is_resolved,
        created_at: timestamp(suggestion.created_at),
    }
}

#[napi(js_name = "createService")]
pub async fn create_service() -> Result<External<Service>> {
    compose_service()
        .await
        .map(External::new)
        .map_err(to_napi_error)
}

#[napi(js_name = "getSecrets")]
pub fn get_secrets(service: &External<Service>) -> Vec<SecretEntryDto> {
    service
        .secrets()
        .iter()
        .map(|(key, value)| SecretEntryDto {
            key: key.clone(),
            value: value.clone(),
        })
        .collect()
}

#[napi(js_name = "authenticateIapRequest")]
pub async fn authenticate_iap_request(
    service: &External<Service>,
    headers: IapRequestHeaders,
) -> Result<Option<AuthenticatedIdentity>> {
    let evidence = ApplicationIapRequestEvidence {
        authenticated_user_email: headers.authenticated_user_email,
        authenticated_user_id: headers.authenticated_user_id,
        jwt_assertion: headers.jwt_assertion,
    };
    service
        .authenticate_iap(&evidence)
        .await
        .map(|identity| identity.map(AuthenticatedIdentity::from))
        .map_err(to_napi_error)
}

#[napi(js_name = "uploadObject")]
pub async fn upload_object(
    service: &External<Service>,
    data: Buffer,
    filename: String,
    content_type: String,
) -> Result<UploadResult> {
    service
        .upload_object(Bytes::from(data.to_vec()), filename, content_type)
        .await
        .map(UploadResult::from)
        .map_err(to_napi_error)
}

#[napi(js_name = "getOrCreateIapUser")]
pub async fn get_or_create_iap_user(
    service: &External<Service>,
    subject: String,
    email: String,
) -> Result<UserDto> {
    let identity = domain::IapIdentity::new(subject, email)
        .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    service
        .users
        .get_or_create_iap_user(&identity)
        .await
        .map(user_dto)
        .map_err(to_napi_error)
}

#[napi(js_name = "createChat")]
pub async fn create_chat(
    service: &External<Service>,
    id: String,
    user_id: String,
    title: String,
    visibility: String,
    created_at: String,
) -> Result<ChatDto> {
    let visibility = domain::Visibility::parse(&visibility)
        .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    let chat = domain::Chat::new(
        id,
        user_id,
        title,
        visibility,
        parse_timestamp(&created_at).map_err(to_napi_error)?,
    )
    .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    service
        .chats
        .create_chat(&chat)
        .await
        .map(chat_dto)
        .map_err(to_napi_error)
}

#[napi(js_name = "getChat")]
pub async fn get_chat(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
) -> Result<Option<ChatDto>> {
    service
        .chats
        .find_chat(&user_id, &chat_id)
        .await
        .map(|chat| chat.map(chat_dto))
        .map_err(to_napi_error)
}

#[napi(js_name = "updateChatTitle")]
pub async fn update_chat_title(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
    title: String,
) -> Result<ChatDto> {
    service
        .chats
        .update_title(&user_id, &chat_id, &title)
        .await
        .map(chat_dto)
        .map_err(to_napi_error)
}

#[napi(js_name = "updateChatVisibility")]
pub async fn update_chat_visibility(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
    visibility: String,
) -> Result<ChatDto> {
    let visibility = domain::Visibility::parse(&visibility)
        .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    service
        .chats
        .update_visibility(&user_id, &chat_id, visibility)
        .await
        .map(chat_dto)
        .map_err(to_napi_error)
}

#[napi(js_name = "deleteChat")]
pub async fn delete_chat(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
) -> Result<ChatDto> {
    service
        .chats
        .delete_chat(&user_id, &chat_id)
        .await
        .map(chat_dto)
        .map_err(to_napi_error)
}

#[napi(js_name = "deleteAllChats")]
pub async fn delete_all_chats(service: &External<Service>, user_id: String) -> Result<i64> {
    service
        .chats
        .delete_all_chats_by_user(&user_id)
        .await
        .map(|count| count as i64)
        .map_err(to_napi_error)
}

#[napi(js_name = "getChatHistory")]
pub async fn get_chat_history(
    service: &External<Service>,
    user_id: String,
    limit: i64,
    starting_after: Option<String>,
    ending_before: Option<String>,
) -> Result<ChatHistoryDto> {
    let starting_after = starting_after
        .map(ChatHistoryCursor::decode)
        .transpose()
        .map_err(|error| {
            to_napi_error(ServiceError::Chat(
                crate::application::chat_service::ChatServiceError::Persistence(error),
            ))
        })?;
    let ending_before = ending_before
        .map(ChatHistoryCursor::decode)
        .transpose()
        .map_err(|error| {
            to_napi_error(ServiceError::Chat(
                crate::application::chat_service::ChatServiceError::Persistence(error),
            ))
        })?;
    let query = NativeChatHistoryQuery::new(user_id, limit as u32, starting_after, ending_before)
        .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    service
        .chats
        .history(&query)
        .await
        .map(|page| {
            let next_cursor = page
                .chats
                .last()
                .and_then(|chat| ChatHistoryCursor::new(chat.position()).encode().ok());
            ChatHistoryDto {
                chats: page.chats.into_iter().map(chat_dto).collect(),
                has_more: page.has_more,
                next_cursor,
            }
        })
        .map_err(to_napi_error)
}

#[napi(js_name = "voteMessage")]
pub async fn vote_message(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
    message_id: String,
    is_upvoted: bool,
) -> Result<VoteDto> {
    let vote = domain::Vote::new(chat_id, message_id, is_upvoted)
        .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    service
        .chats
        .upsert_vote(&user_id, &vote)
        .await
        .map(|vote| VoteDto {
            chat_id: vote.chat_id,
            message_id: vote.message_id,
            is_upvoted: vote.is_upvoted,
        })
        .map_err(to_napi_error)
}

#[napi(js_name = "getVotes")]
pub async fn get_votes(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
) -> Result<Vec<VoteDto>> {
    service
        .chats
        .list_votes(&user_id, &chat_id)
        .await
        .map(|votes| {
            votes
                .into_iter()
                .map(|vote| VoteDto {
                    chat_id: vote.chat_id,
                    message_id: vote.message_id,
                    is_upvoted: vote.is_upvoted,
                })
                .collect()
        })
        .map_err(to_napi_error)
}

#[napi(js_name = "createStream")]
pub async fn create_stream(
    service: &External<Service>,
    user_id: String,
    stream_id: String,
    chat_id: String,
    created_at: String,
) -> Result<StreamDto> {
    let stream = domain::Stream::new(
        stream_id,
        chat_id,
        parse_timestamp(&created_at).map_err(to_napi_error)?,
    )
    .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    service
        .chats
        .create_stream(&user_id, &stream)
        .await
        .map(|stream| StreamDto {
            id: stream.id,
            chat_id: stream.chat_id,
            created_at: timestamp(stream.created_at),
        })
        .map_err(to_napi_error)
}

#[napi(js_name = "getStreams")]
pub async fn get_streams(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
) -> Result<Vec<String>> {
    service
        .chats
        .list_streams(&user_id, &chat_id)
        .await
        .map(|streams| streams.into_iter().map(|stream| stream.id).collect())
        .map_err(to_napi_error)
}

fn message_from_input(input: MessageInput) -> StdResult<domain::Message, ServiceError> {
    let role = domain::MessageRole::parse(&input.role)
        .map_err(|error| boundary_error(error.to_string()))?;
    domain::Message::new(
        input.id,
        input.chat_id,
        input.user_id,
        role,
        parse_json(&input.parts)?,
        parse_json(&input.attachments)?,
        parse_timestamp(&input.created_at)?,
    )
    .map_err(|error| boundary_error(error.to_string()))
}

#[napi(js_name = "saveMessages")]
pub async fn save_messages(
    service: &External<Service>,
    inputs: Vec<MessageInput>,
) -> Result<Vec<MessageDto>> {
    let messages = inputs
        .into_iter()
        .map(message_from_input)
        .collect::<StdResult<Vec<_>, _>>()
        .map_err(to_napi_error)?;
    service
        .messages
        .save_messages(&messages)
        .await
        .map_err(to_napi_error)?
        .into_iter()
        .map(message_dto)
        .collect::<StdResult<Vec<_>, _>>()
        .map_err(to_napi_error)
}

#[napi(js_name = "updateMessage")]
pub async fn update_message(
    service: &External<Service>,
    input: MessageInput,
) -> Result<MessageDto> {
    let message = message_from_input(input).map_err(to_napi_error)?;
    let message = service
        .messages
        .update_message(&message)
        .await
        .map_err(to_napi_error)?;
    message_dto(message).map_err(to_napi_error)
}

#[napi(js_name = "getMessage")]
pub async fn get_message(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
    message_id: String,
) -> Result<Option<MessageDto>> {
    service
        .messages
        .get_message_by_id(&user_id, &chat_id, &message_id)
        .await
        .map_err(to_napi_error)?
        .map(message_dto)
        .transpose()
        .map_err(to_napi_error)
}

#[napi(js_name = "getMessages")]
pub async fn get_messages(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
) -> Result<Vec<MessageDto>> {
    service
        .messages
        .get_messages_by_chat_id(&user_id, &chat_id)
        .await
        .map_err(to_napi_error)?
        .into_iter()
        .map(message_dto)
        .collect::<StdResult<Vec<_>, _>>()
        .map_err(to_napi_error)
}

#[napi(js_name = "getMessageCount")]
pub async fn get_message_count(
    service: &External<Service>,
    user_id: String,
    cutoff: String,
) -> Result<i64> {
    service
        .messages
        .count_user_messages(&user_id, parse_timestamp(&cutoff).map_err(to_napi_error)?)
        .await
        .map(|count| count as i64)
        .map_err(to_napi_error)
}

#[napi(js_name = "deleteMessagesAfter")]
pub async fn delete_messages_after(
    service: &External<Service>,
    user_id: String,
    chat_id: String,
    cutoff: String,
) -> Result<Vec<MessageDto>> {
    service
        .messages
        .delete_messages_after(
            &user_id,
            &chat_id,
            parse_timestamp(&cutoff).map_err(to_napi_error)?,
        )
        .await
        .map_err(to_napi_error)?
        .into_iter()
        .map(message_dto)
        .collect::<StdResult<Vec<_>, _>>()
        .map_err(to_napi_error)
}

#[napi(js_name = "createDocument")]
pub async fn create_document(
    service: &External<Service>,
    id: String,
    user_id: String,
    title: String,
    kind: String,
    content: String,
) -> Result<DocumentDto> {
    let kind = domain::ArtifactKind::parse(&kind)
        .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    let now = Utc::now();
    let artifact = domain::Artifact::new(id.clone(), user_id.clone(), title, kind, None, now)
        .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    let artifact = match service.artifacts.create_artifact(&artifact).await {
        Ok(artifact) => artifact,
        Err(NativeArtifactServiceError::Persistence(NativePersistenceError::Conflict)) => service
            .artifacts
            .find_artifact(&user_id, &id)
            .await
            .map_err(to_napi_error)?
            .ok_or_else(|| {
                to_napi_error(ServiceError::Artifact(
                    NativeArtifactServiceError::Persistence(NativePersistenceError::Conflict),
                ))
            })?,
        Err(error) => return Err(to_napi_error(ServiceError::Artifact(error))),
    };
    let version = domain::DocumentVersion::new(
        Uuid::new_v4().to_string(),
        id,
        now,
        Some(Value::String(content)),
    )
    .map_err(|error| to_napi_error(boundary_error(error.to_string())))?;
    service
        .artifacts
        .save_document_version(&user_id, &version)
        .await
        .map_err(to_napi_error)
        .and_then(|version| document_dto(&artifact, version).map_err(to_napi_error))
}

#[napi(js_name = "getDocuments")]
pub async fn get_documents(
    service: &External<Service>,
    user_id: String,
    document_id: String,
) -> Result<Vec<DocumentDto>> {
    let artifact = service
        .artifacts
        .find_artifact(&user_id, &document_id)
        .await
        .map_err(to_napi_error)?
        .ok_or_else(|| {
            to_napi_error(ServiceError::Artifact(
                NativeArtifactServiceError::Persistence(NativePersistenceError::NotFound),
            ))
        })?;
    service
        .artifacts
        .get_document_versions(&user_id, &document_id)
        .await
        .map_err(to_napi_error)?
        .into_iter()
        .map(|version| document_dto(&artifact, version).map_err(to_napi_error))
        .collect()
}

#[napi(js_name = "getDocument")]
pub async fn get_document(
    service: &External<Service>,
    user_id: String,
    document_id: String,
) -> Result<Option<DocumentDto>> {
    let Some(artifact) = service
        .artifacts
        .find_artifact(&user_id, &document_id)
        .await
        .map_err(to_napi_error)?
    else {
        return Ok(None);
    };
    service
        .artifacts
        .get_latest_document_version(&user_id, &document_id)
        .await
        .map_err(to_napi_error)?
        .map(|version| document_dto(&artifact, version).map_err(to_napi_error))
        .transpose()
}

#[napi(js_name = "deleteDocumentsAfter")]
pub async fn delete_documents_after(
    service: &External<Service>,
    user_id: String,
    document_id: String,
    cutoff: String,
) -> Result<Vec<DocumentDto>> {
    let artifact = service
        .artifacts
        .find_artifact(&user_id, &document_id)
        .await
        .map_err(to_napi_error)?
        .ok_or_else(|| {
            to_napi_error(ServiceError::Artifact(
                NativeArtifactServiceError::Persistence(NativePersistenceError::NotFound),
            ))
        })?;
    service
        .artifacts
        .delete_document_versions_after(
            &user_id,
            &document_id,
            parse_timestamp(&cutoff).map_err(to_napi_error)?,
        )
        .await
        .map_err(to_napi_error)?
        .into_iter()
        .map(|version| document_dto(&artifact, version).map_err(to_napi_error))
        .collect()
}

#[napi(js_name = "saveSuggestions")]
pub async fn save_suggestions(
    service: &External<Service>,
    inputs: Vec<SuggestionInput>,
) -> Result<Vec<SuggestionDto>> {
    let suggestions = inputs
        .into_iter()
        .map(|input| {
            domain::Suggestion::new(
                input.id,
                input.document_id,
                input.version_id,
                input.user_id,
                input.original_text,
                input.suggested_text,
                input.description,
                parse_timestamp(&input.created_at)?,
            )
            .map(|suggestion| suggestion.with_resolved(input.is_resolved))
            .map_err(|error| boundary_error(error.to_string()))
        })
        .collect::<StdResult<Vec<_>, _>>()
        .map_err(to_napi_error)?;
    service
        .artifacts
        .save_suggestions(
            suggestions
                .first()
                .map(|suggestion| suggestion.user_id.as_str())
                .unwrap_or_default(),
            &suggestions,
        )
        .await
        .map_err(to_napi_error)
        .map(|suggestions| suggestions.into_iter().map(suggestion_dto).collect())
}

#[napi(js_name = "getSuggestions")]
pub async fn get_suggestions(
    service: &External<Service>,
    user_id: String,
    document_id: String,
) -> Result<Vec<SuggestionDto>> {
    service
        .artifacts
        .get_suggestions_by_document_id(&user_id, &document_id)
        .await
        .map(|suggestions| suggestions.into_iter().map(suggestion_dto).collect())
        .map_err(to_napi_error)
}
