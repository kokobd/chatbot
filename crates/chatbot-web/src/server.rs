use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    extract::FromRef,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use chatbot_core::{
    application::{
        chat_service::{ChatService, ChatServiceError},
        file_upload::{FileUploadError, FileUploadService, UploadResult},
        iap_authentication::IapAuthenticationService,
        iap_identity::IapRequestEvidence,
        language_model::{
            LanguageModel, LanguageModelError, ModelContentPart, ModelGenerationRequest,
            ModelMessage, ModelRole, ModelStreamEvent, ModelUsage,
        },
        message_service::{MessageService, MessageServiceError},
        repository::{ChatHistoryCursor, ChatHistoryQuery, PersistenceError},
        secrets::SecretLoader,
        user_service::{UserService, UserServiceError},
    },
    domain::{Chat, IapIdentity, Message, MessageRole, Visibility},
};
use chatbot_infrastructure::infrastructure::{
    firestore::connect, firestore_chat_repository::FirestoreChatRepository,
    firestore_message_repository::FirestoreMessageRepository,
    firestore_user_repository::FirestoreUserRepository, gcs_object_storage::GcsObjectStorage,
    gcs_secret_store::GcsSecretObjectStore, iap_google_identity::GoogleIapIdentityProvider,
    iap_test_identity::TestIapIdentityProvider, openrouter::OpenRouter,
};
use chatbot_protocol::{
    ApiError, ApiErrorCode, ChatHistoryResponse, ChatMessage, ChatResponse, ChatStreamEvent,
    ChatStreamRequest, EditMessageRequest, MessagePart, MessageRole as ProtocolRole,
    ModelCapabilities, ModelInfo, ModelsResponse, UploadResponse, Usage,
};
use chrono::Utc;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::frontend;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    leptos::mount::mount_to_body(frontend::App);
}

const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;
const MAX_MESSAGES_PER_HOUR: u64 = 100;

struct CancellationOnDrop(CancellationToken);

impl Drop for CancellationOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<IapAuthenticationService>,
    pub users: Arc<UserService>,
    pub chats: Arc<ChatService>,
    pub messages: Arc<MessageService>,
    pub uploads: Arc<FileUploadService<GcsObjectStorage>>,
    pub model: Arc<dyn LanguageModel>,
    pub models: Arc<Vec<ModelInfo>>,
    #[cfg(feature = "ssr")]
    pub leptos_options: Option<leptos::config::LeptosOptions>,
}

#[cfg(feature = "ssr")]
impl FromRef<AppState> for leptos::config::LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state
            .leptos_options
            .clone()
            .expect("Leptos options must be configured before serving")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("server error: {0}")]
    Server(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub project_id: String,
    pub firestore_database_id: String,
    pub gcs_bucket: String,
    pub iap_provider: String,
    pub iap_audience: Option<String>,
    pub iap_issuer: String,
    pub openrouter_api_key: Option<String>,
    pub secrets_gcs_path: Option<String>,
    pub openrouter_http_referer: Option<String>,
    pub openrouter_app_name: Option<String>,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, ServerError> {
        let required = |name: &str| {
            std::env::var(name)
                .map_err(|_| ServerError::Configuration(format!("{name} must be configured")))
        };
        let firestore_database_id = required("FIRESTORE_DATABASE_ID")?;
        if firestore_database_id == "(default)" {
            return Err(ServerError::Configuration(
                "FIRESTORE_DATABASE_ID must identify a named database".into(),
            ));
        }
        Ok(Self {
            port: std::env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
            project_id: required("FIRESTORE_PROJECT_ID")?,
            firestore_database_id,
            gcs_bucket: required("GCS_BUCKET")?,
            iap_provider: std::env::var("IAP_AUTH_PROVIDER").unwrap_or_else(|_| "google".into()),
            iap_audience: std::env::var("IAP_JWT_AUDIENCE").ok(),
            iap_issuer: std::env::var("IAP_JWT_ISSUER")
                .unwrap_or_else(|_| "https://cloud.google.com/iap".into()),
            openrouter_api_key: std::env::var("OPENROUTER_API_KEY").ok(),
            secrets_gcs_path: std::env::var("SECRETS_GCS_PATH").ok(),
            openrouter_http_referer: std::env::var("OPENROUTER_HTTP_REFERER").ok(),
            openrouter_app_name: std::env::var("OPENROUTER_APP_NAME").ok(),
        })
    }
}

pub async fn build_state(config: ServerConfig) -> Result<AppState, ServerError> {
    let secret_values = if let Some(path) = config.secrets_gcs_path.as_deref() {
        let store = GcsSecretObjectStore::new()
            .await
            .map_err(|error| ServerError::Configuration(format!("secret store: {error}")))?;
        SecretLoader::new(store)
            .load(path)
            .await
            .map_err(|error| ServerError::Configuration(format!("secret loading: {error}")))?
    } else {
        Default::default()
    };
    let openrouter_api_key = config
        .openrouter_api_key
        .or_else(|| secret_values.get("OPENROUTER_API_KEY").cloned())
        .ok_or_else(|| {
            ServerError::Configuration(
                "OPENROUTER_API_KEY or a secret-object value with that key is required".into(),
            )
        })?;
    let db = connect(&config.project_id, &config.firestore_database_id)
        .await
        .map_err(|error| ServerError::Configuration(format!("Firestore: {error}")))?;
    let users = Arc::new(UserService::new(Arc::new(FirestoreUserRepository::new(
        db.clone(),
    ))));
    let chats = Arc::new(ChatService::new(Arc::new(FirestoreChatRepository::new(
        db.clone(),
    ))));
    let messages = Arc::new(MessageService::new(Arc::new(
        FirestoreMessageRepository::new(db),
    )));
    let identity: Arc<dyn chatbot_core::application::iap_identity::IapIdentityProvider> =
        match config.iap_provider.as_str() {
            "test" if std::env::var_os("K_SERVICE").is_none() => Arc::new(TestIapIdentityProvider),
            "google" => Arc::new(
                GoogleIapIdentityProvider::new(
                    config.iap_audience.ok_or_else(|| {
                        ServerError::Configuration("IAP_JWT_AUDIENCE is required".into())
                    })?,
                    config.iap_issuer,
                    Duration::from_secs(5),
                )
                .map_err(|error| ServerError::Configuration(error.to_string()))?,
            ),
            _ => {
                return Err(ServerError::Configuration(
                    "test IAP is only allowed outside Cloud Run".into(),
                ))
            }
        };
    let auth = Arc::new(IapAuthenticationService::new(identity));
    let storage = GcsObjectStorage::new(config.gcs_bucket)
        .await
        .map_err(|error| ServerError::Configuration(error.to_string()))?;
    let uploads = Arc::new(FileUploadService::new(storage));
    let model = OpenRouter::new(
        openrouter_api_key,
        config.openrouter_http_referer,
        config.openrouter_app_name,
        Duration::from_secs(50),
        Duration::from_secs(30),
    )
    .map_err(|error| ServerError::Configuration(error.to_string()))?;
    Ok(AppState {
        auth,
        users,
        chats,
        messages,
        uploads,
        model: Arc::new(model),
        models: Arc::new(curated_models()),
        #[cfg(feature = "ssr")]
        leptos_options: None,
    })
}

pub async fn run() -> Result<(), ServerError> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| {
            ServerError::Configuration(
                "Rustls crypto provider was initialized before application startup".into(),
            )
        })?;

    let config = ServerConfig::from_env()?;
    let port = config.port;
    let mut state = build_state(config).await?;
    #[cfg(feature = "ssr")]
    {
        state.leptos_options = Some(
            leptos::config::LeptosOptions::builder()
                .output_name("chatbot-web")
                .build(),
        );
    }
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    let mut app = api_routes().nest_service(
        "/assets",
        ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")),
    );
    #[cfg(feature = "ssr")]
    {
        use leptos_axum::{generate_route_list, LeptosRoutes};
        let routes = generate_route_list(frontend::App);
        app = app.leptos_routes(&state, routes, frontend::App);
    }
    axum::serve(listener, app.with_state(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

pub fn router(state: AppState) -> Router {
    api_routes().with_state(state)
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health/ready", get(ready))
        .route("/api/models", get(models))
        .route("/api/chats", get(history).delete(delete_all_chats))
        .route("/api/chats/{id}", get(chat).delete(delete_chat))
        .route("/api/chats/{id}/messages/stream", post(stream_message))
        .route("/api/chats/{id}/messages/edit", post(edit_message))
        .route("/api/uploads", post(upload))
        .layer(TraceLayer::new_for_http())
}

async fn ready() -> &'static str {
    "ready"
}

async fn models(State(state): State<AppState>) -> Json<ModelsResponse> {
    Json(ModelsResponse {
        models: (*state.models).clone(),
    })
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    limit: Option<u32>,
    before: Option<String>,
}

async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let identity = match authenticate(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let user = match get_user(&state, &identity).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let cursor = match query.before.map(ChatHistoryCursor::decode).transpose() {
        Ok(cursor) => cursor,
        Err(error) => {
            return api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                ApiErrorCode::BadRequest,
                error.to_string(),
                false,
            )
        }
    };
    let query = match ChatHistoryQuery::new(user.id, query.limit.unwrap_or(20), None, cursor) {
        Ok(query) => query,
        Err(error) => {
            return api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                ApiErrorCode::BadRequest,
                error.to_string(),
                false,
            )
        }
    };
    match state.chats.history(&query).await {
        Ok(page) => {
            let next_cursor = page
                .chats
                .last()
                .map(|chat| ChatHistoryCursor::new(chat.position()))
                .map(|cursor| cursor.encode())
                .transpose()
                .unwrap_or_default();
            Json(ChatHistoryResponse {
                chats: page.chats.iter().map(chat_summary).collect(),
                has_more: page.has_more,
                next_cursor,
            })
            .into_response()
        }
        Err(error) => map_chat_error(error),
    }
}

async fn chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let identity = match authenticate(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let user = match get_user(&state, &identity).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    match state.chats.find_chat(&user.id, &id).await {
        Ok(Some(_chat)) => match state.messages.get_messages_by_chat_id(&user.id, &id).await {
            Ok(messages) => Json(ChatResponse {
                id,
                messages: messages.iter().filter_map(protocol_message).collect(),
            })
            .into_response(),
            Err(error) => map_message_error(error),
        },
        Ok(None) => not_found(),
        Err(error) => map_chat_error(error),
    }
}

async fn delete_all_chats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let identity = match authenticate(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let user = match get_user(&state, &identity).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    match state.chats.delete_all_chats_by_user(&user.id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => map_chat_error(error),
    }
}

async fn delete_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let identity = match authenticate(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let user = match get_user(&state, &identity).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    match state.chats.delete_chat(&user.id, &id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => map_chat_error(error),
    }
}

async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let identity = match authenticate(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let _user = match get_user(&state, &identity).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let mut filename = None;
    let mut content_type = None;
    let mut data = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    ApiErrorCode::BadRequest,
                    "invalid multipart upload".into(),
                    false,
                )
            }
        };
        if field.name() != Some("file") {
            continue;
        }
        filename = field.file_name().map(str::to_string);
        content_type = field.content_type().map(str::to_string);
        let mut field = field;
        while let Some(chunk) = match field.chunk().await {
            Ok(chunk) => chunk,
            Err(_) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    ApiErrorCode::BadRequest,
                    "invalid multipart upload".into(),
                    false,
                )
            }
        } {
            if data.len() + chunk.len() > MAX_UPLOAD_BYTES {
                return api_error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    ApiErrorCode::BadRequest,
                    "upload exceeds 5 MiB".into(),
                    false,
                );
            }
            data.extend_from_slice(&chunk);
        }
    }
    let Some(filename) = filename else {
        return api_error(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::BadRequest,
            "file is required".into(),
            false,
        );
    };
    let content_type = content_type.unwrap_or_default();
    if !matches!(content_type.as_str(), "image/jpeg" | "image/png") {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::BadRequest,
            "only JPEG and PNG uploads are supported".into(),
            false,
        );
    }
    if !valid_image_signature(&data, &content_type) {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::BadRequest,
            "image content does not match its MIME type".into(),
            false,
        );
    }
    match state
        .uploads
        .upload_for_user(
            Some(&identity.subject),
            Bytes::from(data),
            filename.clone(),
            content_type.clone(),
        )
        .await
    {
        Ok(UploadResult {
            url,
            pathname,
            content_type,
        }) => Json(UploadResponse {
            pathname,
            url,
            filename,
            content_type,
        })
        .into_response(),
        Err(error) => map_upload_error(error),
    }
}

async fn stream_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<String>,
    Json(request): Json<ChatStreamRequest>,
) -> Response {
    let identity = match authenticate(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let user = match get_user(&state, &identity).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let Some(model_info) = state
        .models
        .iter()
        .find(|model| model.id == request.model_id)
    else {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::BadRequest,
            "model is not allowed".into(),
            false,
        );
    };
    if request
        .message
        .parts
        .iter()
        .any(|part| matches!(part, MessagePart::Image { .. }))
        && !model_info.capabilities.vision
    {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::BadRequest,
            "selected model does not support images".into(),
            false,
        );
    }
    if let Ok(count) = state
        .messages
        .count_user_messages(&user.id, Utc::now() - chrono::Duration::hours(1))
        .await
    {
        if count >= MAX_MESSAGES_PER_HOUR {
            return api_error(
                StatusCode::TOO_MANY_REQUESTS,
                ApiErrorCode::RateLimit,
                "message rate limit reached".into(),
                true,
            );
        }
    }
    let chat = match state.chats.find_chat(&user.id, &chat_id).await {
        Ok(Some(chat)) => chat,
        Ok(None) => match Chat::new(
            &chat_id,
            &user.id,
            "New chat",
            Visibility::Private,
            Utc::now(),
        ) {
            Ok(chat) => match state.chats.create_chat(&chat).await {
                Ok(chat) => chat,
                Err(error) => return map_chat_error(error),
            },
            Err(error) => {
                return api_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    ApiErrorCode::BadRequest,
                    error.to_string(),
                    false,
                )
            }
        },
        Err(error) => return map_chat_error(error),
    };
    let now = Utc::now();
    let parts = protocol_parts_json(&request.message.parts);
    let user_message = match Message::new(
        &request.message.id,
        &chat.id,
        &user.id,
        MessageRole::User,
        parts.clone(),
        serde_json::json!([]),
        now,
    ) {
        Ok(message) => message,
        Err(error) => {
            return api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                ApiErrorCode::BadRequest,
                error.to_string(),
                false,
            )
        }
    };
    match state.messages.save_messages(&[user_message.clone()]).await {
        Ok(_) => {}
        Err(MessageServiceError::Persistence(PersistenceError::Conflict)) => match state
            .messages
            .get_message_by_id(&user.id, &chat.id, &request.message.id)
            .await
        {
            Ok(Some(existing)) if existing.parts == parts => {}
            Ok(_) => {
                return api_error(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    "message ID already belongs to different content".into(),
                    false,
                )
            }
            Err(error) => return map_message_error(error),
        },
        Err(error) => return map_message_error(error),
    }
    let prior = match state
        .messages
        .get_messages_by_chat_id(&user.id, &chat.id)
        .await
    {
        Ok(messages) => messages.into_iter().filter_map(model_message).collect(),
        Err(error) => return map_message_error(error),
    };
    let cancellation = CancellationToken::new();
    let request_id = Uuid::new_v4().to_string();
    let model_request = ModelGenerationRequest {
        model_id: request.model_id.clone(),
        messages: prior,
        request_id,
        timeout: Duration::from_secs(50),
        cancellation: cancellation.clone(),
    };
    let model = state.model.clone();
    let messages = state.messages.clone();
    let user_id = user.id.clone();
    let chat_id = chat.id.clone();
    let message_id = Uuid::new_v4().to_string();
    let stream = async_stream::stream! {
        let _cancellation_on_drop = CancellationOnDrop(cancellation);
        yield Ok::<Event, Infallible>(sse_event(ChatStreamEvent::Status { phase: "waiting".into(), message: "Waiting...".into() }));
        let response = model.stream(model_request).await;
        match response {
            Ok(mut stream) => {
                let mut text = String::new();
                let mut usage = None;
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(ModelStreamEvent::TextDelta(delta)) => { text.push_str(&delta); yield Ok(sse_event(ChatStreamEvent::TextDelta { delta })); }
                        Ok(ModelStreamEvent::ReasoningDelta(delta)) => { yield Ok(sse_event(ChatStreamEvent::ReasoningDelta { delta })); }
                        Ok(ModelStreamEvent::Usage(value)) => usage = Some(value),
                        Err(error) => { yield Ok(sse_event(ChatStreamEvent::Error(model_error(error)))); return; }
                    }
                }
                let message = match Message::new(message_id, chat_id, user_id, MessageRole::Assistant, serde_json::json!([{"type":"text","text":text}]), serde_json::json!([]), Utc::now()) {
                    Ok(message) => message,
                    Err(_) => { yield Ok(sse_event(ChatStreamEvent::Error(ApiError { code: ApiErrorCode::Internal, message: "assistant response was invalid".into(), retryable: false }))); return; }
                };
                match messages.save_messages(&[message.clone()]).await {
                    Ok(_) => yield Ok(sse_event(ChatStreamEvent::Complete { message: protocol_message(&message).unwrap(), usage: usage.map(protocol_usage) })),
                    Err(error) => yield Ok(sse_event(ChatStreamEvent::Error(ApiError { code: ApiErrorCode::Internal, message: "assistant response could not be saved".into(), retryable: matches!(error, MessageServiceError::Persistence(PersistenceError::Unavailable { retryable: true, .. })) }))),
                }
            }
            Err(error) => yield Ok(sse_event(ChatStreamEvent::Error(model_error(error)))),
        }
    };
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(10))
                .text("keep-alive"),
        )
        .into_response()
}

async fn edit_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<String>,
    Json(request): Json<EditMessageRequest>,
) -> Response {
    let identity = match authenticate(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let user = match get_user(&state, &identity).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let Some(message) = state
        .messages
        .get_message_by_id(&user.id, &chat_id, &request.message_id)
        .await
        .ok()
        .flatten()
    else {
        return not_found();
    };
    let messages = match state
        .messages
        .get_messages_by_chat_id(&user.id, &chat_id)
        .await
    {
        Ok(messages) => messages,
        Err(error) => return map_message_error(error),
    };
    let branch_count = messages
        .iter()
        .filter(|candidate| candidate.position() >= message.position())
        .count();
    if branch_count > 500 {
        return api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            ApiErrorCode::BadRequest,
            "message branch exceeds 500 messages".into(),
            false,
        );
    }
    match state
        .messages
        .delete_messages_from(&user.id, &chat_id, &message.position())
        .await
    {
        Ok(count) => Json(serde_json::json!({"deleted": count})).into_response(),
        Err(error) => map_message_error(error),
    }
}

async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<chatbot_core::application::iap_identity::AuthenticatedIdentity, Response> {
    let evidence = IapRequestEvidence {
        jwt_assertion: headers
            .get("x-goog-iap-jwt-assertion")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        authenticated_user_email: headers
            .get("x-goog-authenticated-user-email")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        authenticated_user_id: headers
            .get("x-goog-authenticated-user-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
    };
    match state.auth.authenticate(&evidence).await {
        Ok(Some(identity)) => Ok(identity),
        Ok(None) => Err(api_error(
            StatusCode::UNAUTHORIZED,
            ApiErrorCode::Unauthorized,
            "authentication required".into(),
            false,
        )),
        Err(error) => Err(api_error(
            StatusCode::UNAUTHORIZED,
            ApiErrorCode::Unauthorized,
            error.to_string(),
            false,
        )),
    }
}

async fn get_user(
    state: &AppState,
    identity: &chatbot_core::application::iap_identity::AuthenticatedIdentity,
) -> Result<chatbot_core::domain::User, Response> {
    let identity = match IapIdentity::new(&identity.subject, identity.email.clone()) {
        Ok(identity) => identity,
        Err(error) => {
            return Err(api_error(
                StatusCode::UNAUTHORIZED,
                ApiErrorCode::Unauthorized,
                error.to_string(),
                false,
            ))
        }
    };
    state
        .users
        .get_or_create_iap_user(&identity)
        .await
        .map_err(map_user_error)
}

fn protocol_parts_json(parts: &[MessagePart]) -> serde_json::Value {
    serde_json::to_value(parts).unwrap_or_else(|_| serde_json::json!([]))
}
fn model_message(message: Message) -> Option<ModelMessage> {
    let role = match message.role {
        MessageRole::User => ModelRole::User,
        MessageRole::Assistant => ModelRole::Assistant,
        MessageRole::System | MessageRole::Tool => return None,
    };
    let parts = match message.parts {
        serde_json::Value::Array(parts) => parts
            .into_iter()
            .filter_map(
                |part| match part.get("type").and_then(|value| value.as_str()) {
                    Some("image") => Some(ModelContentPart::Image {
                        url: part.get("url")?.as_str()?.to_string(),
                        media_type: part.get("media_type")?.as_str()?.to_string(),
                    }),
                    _ => part
                        .get("text")
                        .and_then(|value| value.as_str())
                        .map(|text| ModelContentPart::Text(text.to_string())),
                },
            )
            .collect(),
        _ => vec![],
    };
    Some(ModelMessage { role, parts })
}
fn protocol_message(message: &Message) -> Option<ChatMessage> {
    Some(ChatMessage {
        id: message.id.clone(),
        role: match message.role {
            MessageRole::User => ProtocolRole::User,
            MessageRole::Assistant => ProtocolRole::Assistant,
            _ => return None,
        },
        parts: serde_json::from_value(message.parts.clone()).ok()?,
        created_at: message.created_at.to_rfc3339(),
    })
}
fn chat_summary(chat: &Chat) -> chatbot_protocol::ChatSummary {
    chatbot_protocol::ChatSummary {
        id: chat.id.clone(),
        title: chat.title.clone(),
        created_at: chat.created_at.to_rfc3339(),
    }
}
fn protocol_usage(usage: ModelUsage) -> Usage {
    Usage {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
    }
}
fn model_error(error: LanguageModelError) -> ApiError {
    let (code, retryable) = match error {
        LanguageModelError::Authentication => (ApiErrorCode::ProviderAuthentication, false),
        LanguageModelError::Credits => (ApiErrorCode::ProviderCredits, false),
        LanguageModelError::RateLimit | LanguageModelError::Timeout => {
            (ApiErrorCode::ProviderUnavailable, true)
        }
        LanguageModelError::Unavailable { retryable, .. } => {
            (ApiErrorCode::ProviderUnavailable, retryable)
        }
        LanguageModelError::InvalidRequest { .. } => (ApiErrorCode::ProviderInvalidRequest, false),
        LanguageModelError::MalformedStream { .. } => (ApiErrorCode::ProviderUnavailable, false),
        LanguageModelError::Unknown { retryable, .. } => {
            (ApiErrorCode::ProviderUnavailable, retryable)
        }
    };
    ApiError {
        code,
        message: "model provider request failed".into(),
        retryable,
    }
}
fn sse_event(event: ChatStreamEvent) -> Event {
    Event::default()
        .event(match &event {
            ChatStreamEvent::Status { .. } => "status",
            ChatStreamEvent::ReasoningDelta { .. } => "reasoning_delta",
            ChatStreamEvent::TextDelta { .. } => "text_delta",
            ChatStreamEvent::Complete { .. } => "complete",
            ChatStreamEvent::Error(_) => "error",
        })
        .json_data(event)
        .expect("protocol event serializes")
}
fn valid_image_signature(bytes: &[u8], content_type: &str) -> bool {
    match content_type {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        _ => false,
    }
}
fn api_error(status: StatusCode, code: ApiErrorCode, message: String, retryable: bool) -> Response {
    (
        status,
        Json(ApiError {
            code,
            message,
            retryable,
        }),
    )
        .into_response()
}
fn not_found() -> Response {
    api_error(
        StatusCode::NOT_FOUND,
        ApiErrorCode::NotFound,
        "resource not found".into(),
        false,
    )
}
fn map_user_error(error: UserServiceError) -> Response {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        ApiErrorCode::Internal,
        error.to_string(),
        false,
    )
}
fn map_chat_error(error: ChatServiceError) -> Response {
    map_persistence(error.to_string(), persistence_from_chat(&error))
}
fn map_message_error(error: MessageServiceError) -> Response {
    map_persistence(error.to_string(), persistence_from_message(&error))
}
fn map_persistence(message: String, error: Option<&PersistenceError>) -> Response {
    match error {
        Some(PersistenceError::NotFound) => not_found(),
        Some(PersistenceError::Conflict) => {
            api_error(StatusCode::CONFLICT, ApiErrorCode::Conflict, message, false)
        }
        Some(PersistenceError::InvalidInput(_)) => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::BadRequest,
            message,
            false,
        ),
        Some(PersistenceError::Unavailable { retryable, .. }) => api_error(
            StatusCode::BAD_GATEWAY,
            ApiErrorCode::Internal,
            message,
            *retryable,
        ),
        _ => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiErrorCode::Internal,
            message,
            false,
        ),
    }
}
fn persistence_from_chat(error: &ChatServiceError) -> Option<&PersistenceError> {
    match error {
        ChatServiceError::Persistence(error) => Some(error),
        _ => None,
    }
}
fn persistence_from_message(error: &MessageServiceError) -> Option<&PersistenceError> {
    match error {
        MessageServiceError::Persistence(error) => Some(error),
        _ => None,
    }
}
fn map_upload_error(error: FileUploadError) -> Response {
    api_error(
        StatusCode::BAD_GATEWAY,
        ApiErrorCode::Internal,
        error.to_string(),
        true,
    )
}
fn curated_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "deepseek/deepseek-v4-flash-0731".into(),
            name: "DeepSeek V4 Flash".into(),
            provider: "deepseek".into(),
            description: "Fast text model for everyday chat".into(),
            capabilities: ModelCapabilities {
                vision: false,
                reasoning: true,
            },
        },
        ModelInfo {
            id: "openai/gpt-5.6-luna".into(),
            name: "GPT 5.6 Luna".into(),
            provider: "openai".into(),
            description: "OpenAI multimodal model".into(),
            capabilities: ModelCapabilities {
                vision: true,
                reasoning: false,
            },
        },
    ]
}
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::{api_routes, valid_image_signature};

    #[test]
    fn builds_api_routes_with_axum_0_8_path_captures() {
        let _router = api_routes();
    }

    #[test]
    fn rejects_mismatched_image_signatures() {
        assert!(!valid_image_signature(b"not png", "image/png"));
        assert!(valid_image_signature(b"\x89PNG\r\n\x1a\nrest", "image/png"));
    }
}
