use async_trait::async_trait;
use chrono::{DateTime, Utc};
use firestore::errors::FirestoreError;
use firestore::timestamp_utils::from_timestamp;
use firestore::{
    FirestoreDb, FirestoreGetByIdSupport, FirestoreQueryCursor, FirestoreQueryDirection,
    FirestoreWritePrecondition,
};
use serde::{Deserialize, Serialize};

use crate::application::repository::chat_repository::{
    ChatHistoryPage, ChatHistoryQuery, ChatRepository, ChatTitle,
};
use crate::application::repository::error::PersistenceError;
use crate::domain::{Chat, LifecycleState, Visibility};

pub const CHATS_COLLECTION: &str = "chats";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ChatDocument {
    pub(crate) id: String,
    #[serde(rename = "userId")]
    pub(crate) user_id: String,
    pub(crate) title: String,
    pub(crate) visibility: String,
    pub(crate) lifecycle: String,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: DateTime<Utc>,
    #[serde(rename = "deletedAt")]
    pub(crate) deleted_at: Option<DateTime<Utc>>,
    #[serde(rename = "lifecycleRevision")]
    pub(crate) lifecycle_revision: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ChatTitleUpdate {
    title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ChatVisibilityUpdate {
    visibility: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ChatDeleteUpdate {
    lifecycle: String,
    #[serde(rename = "deletedAt")]
    deleted_at: Option<DateTime<Utc>>,
    #[serde(rename = "lifecycleRevision")]
    lifecycle_revision: i64,
}

impl ChatDocument {
    fn from_create_chat(chat: &Chat) -> Result<Self, PersistenceError> {
        validate_document_id(&chat.id)?;
        if chat.lifecycle != LifecycleState::Active
            || chat.deleted_at.is_some()
            || chat.lifecycle_revision != 0
        {
            return Err(PersistenceError::InvalidInput(
                "chat create payload must be active and have no tombstone".to_string(),
            ));
        }

        Ok(Self {
            id: chat.id.clone(),
            user_id: chat.user_id.clone(),
            title: chat.title.clone(),
            visibility: serde_json::to_string(&chat.visibility)
                .map_err(|error| PersistenceError::Serialization(error.to_string()))?
                .trim_matches('"')
                .to_string(),
            lifecycle: "active".to_string(),
            created_at: chat.created_at,
            deleted_at: None,
            lifecycle_revision: 0,
        })
    }

    fn into_chat(self, document_id: &str) -> Result<Chat, PersistenceError> {
        validate_document_id(document_id)
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        if self.id != document_id {
            return Err(PersistenceError::CorruptData(
                "chat ID does not match the document ID".to_string(),
            ));
        }
        if self.lifecycle_revision < 0 {
            return Err(PersistenceError::CorruptData(
                "chat lifecycle revision must not be negative".to_string(),
            ));
        }

        let visibility = Visibility::parse(&self.visibility)
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        let lifecycle = LifecycleState::parse(&self.lifecycle)
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        Chat::from_persisted(
            self.id,
            self.user_id,
            self.title,
            visibility,
            lifecycle,
            self.created_at,
            self.deleted_at,
            self.lifecycle_revision as u64,
        )
        .map_err(|error| PersistenceError::CorruptData(error.to_string()))
    }

    fn has_same_create_payload(&self, chat: &Chat) -> bool {
        self.id == chat.id
            && self.user_id == chat.user_id
            && self.title == chat.title
            && self.visibility == visibility_wire(chat.visibility)
            && self.lifecycle == "active"
            && self.created_at == chat.created_at
            && self.deleted_at.is_none()
            && self.lifecycle_revision == 0
    }
}

pub struct FirestoreChatRepository {
    db: FirestoreDb,
}

impl FirestoreChatRepository {
    pub fn new(db: FirestoreDb) -> Self {
        Self { db }
    }

    async fn raw_document(
        &self,
        chat_id: &str,
        operation: FirestoreOperation,
    ) -> Result<Option<(ChatDocument, Option<DateTime<Utc>>)>, PersistenceError> {
        validate_document_id(chat_id)?;
        let document = match self.db.get_doc(CHATS_COLLECTION, chat_id, None).await {
            Ok(document) => document,
            Err(error) => {
                let mapped = map_firestore_error(error, operation);
                if mapped == PersistenceError::NotFound {
                    return Ok(None);
                }
                return Err(mapped);
            }
        };
        let document_id = document
            .name
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| PersistenceError::CorruptData("chat document has no ID".to_string()))?;
        if document_id != chat_id {
            return Err(PersistenceError::CorruptData(
                "chat document path does not match the requested ID".to_string(),
            ));
        }
        let update_time = document
            .update_time
            .clone()
            .map(from_timestamp)
            .transpose()
            .map_err(|error| map_firestore_error(error, operation))?;
        let object = FirestoreDb::deserialize_doc_to::<ChatDocument>(&document)
            .map_err(|error| map_firestore_error(error, operation))?;
        Ok(Some((object, update_time)))
    }

    async fn loaded_chat(
        &self,
        chat_id: &str,
        operation: FirestoreOperation,
    ) -> Result<Option<(Chat, Option<DateTime<Utc>>)>, PersistenceError> {
        self.raw_document(chat_id, operation)
            .await?
            .map(|(document, update_time)| {
                document.into_chat(chat_id).map(|chat| (chat, update_time))
            })
            .transpose()
    }

    async fn owned_chat(
        &self,
        user_id: &str,
        chat_id: &str,
        operation: FirestoreOperation,
    ) -> Result<Option<(Chat, Option<DateTime<Utc>>)>, PersistenceError> {
        validate_document_id(chat_id)?;
        crate::domain::validate_identifier(user_id).map_err(PersistenceError::from)?;
        Ok(self
            .loaded_chat(chat_id, operation)
            .await?
            .filter(|(chat, _)| chat.user_id == user_id))
    }

    async fn reconcile_create(
        &self,
        chat: &Chat,
        original: PersistenceError,
    ) -> Result<Chat, PersistenceError> {
        match self
            .loaded_chat(&chat.id, FirestoreOperation::ReconciliationRead)
            .await
        {
            Ok(Some((stored, _))) if stored == *chat => Ok(stored),
            Ok(Some(_)) | Ok(None) => Err(original),
            Err(error) => Err(original.with_reconciliation_failure(error)),
        }
    }

    async fn reconcile_update(
        &self,
        user_id: &str,
        chat_id: &str,
        intended: IntendedUpdate<'_>,
        original: PersistenceError,
    ) -> Result<Chat, PersistenceError> {
        match self
            .loaded_chat(chat_id, FirestoreOperation::ReconciliationRead)
            .await
        {
            Ok(Some((chat, _))) if chat.user_id == user_id && intended.matches(&chat) => Ok(chat),
            Ok(Some(_)) | Ok(None) => Err(original),
            Err(error) => Err(original.with_reconciliation_failure(error)),
        }
    }

    async fn reconcile_delete(
        &self,
        user_id: &str,
        chat_id: &str,
        intended_revision: u64,
        original: PersistenceError,
    ) -> Result<Chat, PersistenceError> {
        match self
            .loaded_chat(chat_id, FirestoreOperation::ReconciliationRead)
            .await
        {
            Ok(Some((chat, _)))
                if chat.user_id == user_id
                    && chat.lifecycle == LifecycleState::Deleted
                    && chat.lifecycle_revision >= intended_revision =>
            {
                Ok(chat)
            }
            Ok(Some(_)) | Ok(None) => Err(original),
            Err(error) => Err(original.with_reconciliation_failure(error)),
        }
    }

    async fn update_title_or_visibility(
        &self,
        user_id: &str,
        chat_id: &str,
        update: ChatFieldUpdate<'_>,
    ) -> Result<Chat, PersistenceError> {
        validate_document_id(chat_id)?;
        crate::domain::validate_identifier(user_id).map_err(PersistenceError::from)?;
        let Some((current, _)) = self
            .owned_chat(user_id, chat_id, FirestoreOperation::UpdateRead)
            .await?
        else {
            return Err(PersistenceError::NotFound);
        };
        if current.lifecycle == LifecycleState::Deleted {
            return Err(PersistenceError::FailedPrecondition(
                "chat is marked for deletion".to_string(),
            ));
        }

        let (fields, object, intended) = match update {
            ChatFieldUpdate::Title(title) => (
                vec!["title"],
                ChatUpdateObject::Title(ChatTitleUpdate {
                    title: title.as_str().to_string(),
                }),
                IntendedUpdate::Title(title.as_str()),
            ),
            ChatFieldUpdate::Visibility(visibility) => (
                vec!["visibility"],
                ChatUpdateObject::Visibility(ChatVisibilityUpdate {
                    visibility: visibility_wire(visibility),
                }),
                IntendedUpdate::Visibility(visibility),
            ),
        };

        let commit_result = match object {
            ChatUpdateObject::Title(object) => {
                self.db
                    .fluent()
                    .update()
                    .fields(fields)
                    .in_col(CHATS_COLLECTION)
                    .document_id(chat_id)
                    .object(&object)
                    .execute::<ChatDocument>()
                    .await
            }
            ChatUpdateObject::Visibility(object) => {
                self.db
                    .fluent()
                    .update()
                    .fields(fields)
                    .in_col(CHATS_COLLECTION)
                    .document_id(chat_id)
                    .object(&object)
                    .execute::<ChatDocument>()
                    .await
            }
        };

        match commit_result {
            Ok(_) => {
                self.reconcile_post_commit_update(user_id, chat_id, intended)
                    .await
            }
            Err(error) => {
                let mapped = map_firestore_error(error, FirestoreOperation::UpdateCommit);
                if is_ambiguous_write(&mapped) {
                    self.reconcile_update(user_id, chat_id, intended, mapped)
                        .await
                } else {
                    Err(mapped)
                }
            }
        }
    }

    async fn reconcile_post_commit_update(
        &self,
        user_id: &str,
        chat_id: &str,
        intended: IntendedUpdate<'_>,
    ) -> Result<Chat, PersistenceError> {
        let original = unknown_outcome(
            "chat update committed but its response was not readable",
            false,
        );
        self.reconcile_update(user_id, chat_id, intended, original)
            .await
    }
}

enum ChatFieldUpdate<'a> {
    Title(&'a ChatTitle),
    Visibility(Visibility),
}

enum ChatUpdateObject {
    Title(ChatTitleUpdate),
    Visibility(ChatVisibilityUpdate),
}

#[derive(Clone, Copy)]
enum IntendedUpdate<'a> {
    Title(&'a str),
    Visibility(Visibility),
}

impl IntendedUpdate<'_> {
    fn matches(self, chat: &Chat) -> bool {
        match self {
            Self::Title(title) => chat.title == title,
            Self::Visibility(visibility) => chat.visibility == visibility,
        }
    }
}

#[async_trait]
impl ChatRepository for FirestoreChatRepository {
    async fn find_chat(
        &self,
        user_id: &str,
        chat_id: &str,
    ) -> Result<Option<Chat>, PersistenceError> {
        Ok(self
            .owned_chat(user_id, chat_id, FirestoreOperation::Read)
            .await?
            .map(|(chat, _)| chat)
            .filter(|chat| chat.lifecycle != LifecycleState::Deleted))
    }

    async fn create_chat(&self, chat: &Chat) -> Result<Chat, PersistenceError> {
        let document = ChatDocument::from_create_chat(chat)?;
        let document_id = document.id.clone();
        let document = FirestoreDb::serialize_to_doc("", &document)
            .map_err(|error| map_firestore_error(error, FirestoreOperation::CreateSetup))?;

        match self
            .db
            .fluent()
            .insert()
            .into(CHATS_COLLECTION)
            .document_id(&document_id)
            .document(document)
            .execute()
            .await
        {
            Ok(_) => Ok(chat.clone()),
            Err(error) => {
                let mapped = map_firestore_error(error, FirestoreOperation::CreateCommit);
                if mapped == PersistenceError::Conflict {
                    match self
                        .loaded_chat(&chat.id, FirestoreOperation::ConflictRead)
                        .await
                    {
                        Ok(Some((stored, _)))
                            if ChatDocument::from_create_chat(chat)
                                .map(|document| document.has_same_create_payload(chat))
                                .unwrap_or(false)
                                && stored == *chat =>
                        {
                            Ok(stored)
                        }
                        Ok(Some(_)) | Ok(None) => Err(PersistenceError::Conflict),
                        Err(error) => Err(error),
                    }
                } else if is_ambiguous_write(&mapped) {
                    self.reconcile_create(chat, mapped).await
                } else {
                    Err(mapped)
                }
            }
        }
    }

    async fn update_chat_title(
        &self,
        user_id: &str,
        chat_id: &str,
        title: &ChatTitle,
    ) -> Result<Chat, PersistenceError> {
        self.update_title_or_visibility(user_id, chat_id, ChatFieldUpdate::Title(title))
            .await
    }

    async fn update_chat_visibility(
        &self,
        user_id: &str,
        chat_id: &str,
        visibility: Visibility,
    ) -> Result<Chat, PersistenceError> {
        self.update_title_or_visibility(user_id, chat_id, ChatFieldUpdate::Visibility(visibility))
            .await
    }

    async fn delete_chat(&self, user_id: &str, chat_id: &str) -> Result<Chat, PersistenceError> {
        validate_document_id(chat_id)?;
        crate::domain::validate_identifier(user_id).map_err(PersistenceError::from)?;
        let Some((current, update_time)) = self
            .owned_chat(user_id, chat_id, FirestoreOperation::DeleteRead)
            .await?
        else {
            return Err(PersistenceError::NotFound);
        };
        if current.lifecycle == LifecycleState::Deleted {
            return Ok(current);
        }
        let update_time = update_time.ok_or_else(|| {
            PersistenceError::CorruptData("chat update time is missing".to_string())
        })?;
        let intended_revision = current.lifecycle_revision + 1;
        let update = ChatDeleteUpdate {
            lifecycle: "deleted".to_string(),
            deleted_at: Some(Utc::now()),
            lifecycle_revision: intended_revision as i64,
        };
        let commit_result = self
            .db
            .fluent()
            .update()
            .fields(["lifecycle", "deletedAt", "lifecycleRevision"])
            .in_col(CHATS_COLLECTION)
            .precondition(FirestoreWritePrecondition::UpdateTime(update_time))
            .document_id(chat_id)
            .object(&update)
            .execute::<ChatDocument>()
            .await;

        match commit_result {
            Ok(_) => {
                self.reconcile_post_commit_delete(user_id, chat_id, intended_revision)
                    .await
            }
            Err(error) => {
                let mapped = map_firestore_error(error, FirestoreOperation::DeleteCommit);
                if is_ambiguous_write(&mapped) {
                    self.reconcile_delete(user_id, chat_id, intended_revision, mapped)
                        .await
                } else {
                    Err(mapped)
                }
            }
        }
    }

    async fn list_chat_history(
        &self,
        query: &ChatHistoryQuery,
    ) -> Result<ChatHistoryPage, PersistenceError> {
        crate::domain::validate_identifier(&query.user_id).map_err(PersistenceError::from)?;
        if query.starting_after.is_some() && query.ending_before.is_some() {
            return Err(PersistenceError::InvalidInput(
                "only one history cursor may be provided".to_string(),
            ));
        }

        let mut builder = self
            .db
            .fluent()
            .select()
            .from(CHATS_COLLECTION)
            .filter(|filter| {
                filter.for_all([
                    filter.field("userId").eq(query.user_id.clone()),
                    filter.field("lifecycle").neq("deleted"),
                ])
            })
            .order_by([
                ("createdAt", FirestoreQueryDirection::Descending),
                ("id", FirestoreQueryDirection::Descending),
            ])
            .limit(query.limit + 1);

        if let Some(cursor) = &query.starting_after {
            builder = builder.end_at(FirestoreQueryCursor::BeforeValue(vec![
                cursor.position().created_at.into(),
                cursor.position().id.clone().into(),
            ]));
        }
        if let Some(cursor) = &query.ending_before {
            builder = builder.start_at(FirestoreQueryCursor::AfterValue(vec![
                cursor.position().created_at.into(),
                cursor.position().id.clone().into(),
            ]));
        }

        if let Some(cursor) = query
            .starting_after
            .as_ref()
            .or(query.ending_before.as_ref())
        {
            let anchor = self
                .owned_chat(
                    &query.user_id,
                    &cursor.position().id,
                    FirestoreOperation::AnchorRead,
                )
                .await?
                .filter(|(chat, _)| chat.lifecycle != LifecycleState::Deleted);
            if anchor.is_none() {
                return Err(PersistenceError::NotFound);
            }
        }

        let documents = builder
            .query()
            .await
            .map_err(|error| map_firestore_error(error, FirestoreOperation::HistoryRead))?;
        let mut chats = Vec::with_capacity(documents.len());
        for document in documents {
            let document_id = document
                .name
                .rsplit('/')
                .next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    PersistenceError::CorruptData("chat document has no ID".to_string())
                })?;
            let document = FirestoreDb::deserialize_doc_to::<ChatDocument>(&document)
                .map_err(|error| map_firestore_error(error, FirestoreOperation::HistoryRead))?;
            let chat = document.into_chat(document_id)?;
            if chat.user_id == query.user_id && chat.lifecycle != LifecycleState::Deleted {
                chats.push(chat);
            }
        }

        let has_more = chats.len() > query.limit as usize;
        chats.truncate(query.limit as usize);
        Ok(ChatHistoryPage { chats, has_more })
    }
}

impl FirestoreChatRepository {
    async fn reconcile_post_commit_delete(
        &self,
        user_id: &str,
        chat_id: &str,
        intended_revision: u64,
    ) -> Result<Chat, PersistenceError> {
        let original = unknown_outcome(
            "chat delete committed but its response was not readable",
            false,
        );
        self.reconcile_delete(user_id, chat_id, intended_revision, original)
            .await
    }
}

fn visibility_wire(visibility: Visibility) -> String {
    match visibility {
        Visibility::Public => "public".to_string(),
        Visibility::Private => "private".to_string(),
    }
}

fn validate_document_id(value: &str) -> Result<(), PersistenceError> {
    crate::domain::validate_identifier(value).map_err(PersistenceError::from)?;
    if value.contains('/') {
        return Err(PersistenceError::InvalidInput(
            "chat ID must not contain '/'".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum FirestoreOperation {
    Read,
    CreateSetup,
    CreateCommit,
    ConflictRead,
    UpdateRead,
    UpdateCommit,
    DeleteRead,
    DeleteCommit,
    AnchorRead,
    HistoryRead,
    ReconciliationRead,
}

fn map_firestore_error(error: FirestoreError, operation: FirestoreOperation) -> PersistenceError {
    match error {
        FirestoreError::DataConflictError(_) => match operation {
            FirestoreOperation::UpdateCommit | FirestoreOperation::DeleteCommit => {
                PersistenceError::FailedPrecondition(
                    "Firestore write precondition failed".to_string(),
                )
            }
            _ => PersistenceError::Conflict,
        },
        FirestoreError::DataNotFoundError(_) => PersistenceError::NotFound,
        FirestoreError::SerializeError(error) => match operation {
            FirestoreOperation::CreateCommit
            | FirestoreOperation::UpdateCommit
            | FirestoreOperation::DeleteCommit => unknown_outcome(error.to_string(), false),
            _ => PersistenceError::Serialization(error.to_string()),
        },
        FirestoreError::DeserializeError(error) => match operation {
            FirestoreOperation::CreateCommit
            | FirestoreOperation::UpdateCommit
            | FirestoreOperation::DeleteCommit => unknown_outcome(error.to_string(), false),
            FirestoreOperation::Read
            | FirestoreOperation::ConflictRead
            | FirestoreOperation::UpdateRead
            | FirestoreOperation::DeleteRead
            | FirestoreOperation::AnchorRead
            | FirestoreOperation::HistoryRead
            | FirestoreOperation::ReconciliationRead => {
                PersistenceError::CorruptData(error.to_string())
            }
            FirestoreOperation::CreateSetup => PersistenceError::Serialization(error.to_string()),
        },
        FirestoreError::InvalidParametersError(error) => {
            PersistenceError::InvalidInput(error.to_string())
        }
        FirestoreError::DatabaseError(error) => map_database_error(error, operation),
        FirestoreError::NetworkError(error) => {
            map_unknown_or_unavailable(error.to_string(), operation, true)
        }
        FirestoreError::ErrorInTransaction(error) => {
            map_unknown_or_unavailable(error.to_string(), operation, false)
        }
        FirestoreError::SystemError(error) => map_system_error(error.to_string(), operation),
        FirestoreError::CacheError(error) => PersistenceError::Internal {
            message: error.to_string(),
            retryable: false,
        },
    }
}

fn map_database_error(
    error: firestore::errors::FirestoreDatabaseError,
    operation: FirestoreOperation,
) -> PersistenceError {
    let code = error.public.code.as_str();
    if matches!(
        code,
        "PermissionDenied" | "PERMISSION_DENIED" | "Unauthenticated" | "UNAUTHENTICATED"
    ) {
        return PersistenceError::PermissionDenied(error.to_string());
    }
    if matches!(code, "InvalidArgument" | "INVALID_ARGUMENT") {
        return PersistenceError::InvalidInput(error.to_string());
    }
    if matches!(code, "FailedPrecondition" | "FAILED_PRECONDITION") {
        return PersistenceError::FailedPrecondition(error.to_string());
    }
    map_unknown_or_unavailable(error.to_string(), operation, error.retry_possible)
}

fn map_unknown_or_unavailable(
    message: String,
    operation: FirestoreOperation,
    retryable: bool,
) -> PersistenceError {
    match operation {
        FirestoreOperation::Read
        | FirestoreOperation::ConflictRead
        | FirestoreOperation::UpdateRead
        | FirestoreOperation::DeleteRead
        | FirestoreOperation::AnchorRead
        | FirestoreOperation::HistoryRead
        | FirestoreOperation::ReconciliationRead => {
            if retryable {
                PersistenceError::Unavailable { message, retryable }
            } else {
                PersistenceError::Internal { message, retryable }
            }
        }
        FirestoreOperation::CreateSetup => PersistenceError::Unavailable { message, retryable },
        FirestoreOperation::CreateCommit
        | FirestoreOperation::UpdateCommit
        | FirestoreOperation::DeleteCommit => unknown_outcome(message, retryable),
    }
}

fn map_system_error(message: String, operation: FirestoreOperation) -> PersistenceError {
    match operation {
        FirestoreOperation::CreateCommit
        | FirestoreOperation::UpdateCommit
        | FirestoreOperation::DeleteCommit => unknown_outcome(message, false),
        FirestoreOperation::CreateSetup => PersistenceError::Internal {
            message,
            retryable: false,
        },
        _ => PersistenceError::Internal {
            message,
            retryable: false,
        },
    }
}

fn unknown_outcome(message: impl Into<String>, retryable: bool) -> PersistenceError {
    PersistenceError::OutcomeUnknown {
        message: message.into(),
        retryable,
        reconciliation: None,
    }
}

fn is_ambiguous_write(error: &PersistenceError) -> bool {
    matches!(error, PersistenceError::OutcomeUnknown { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::error::PersistenceError;
    use crate::domain::{PaginationPosition, Visibility};
    use chrono::TimeZone;
    use firestore::errors::{
        FirestoreDatabaseError, FirestoreError, FirestoreErrorPublicGenericDetails,
    };

    fn chat() -> Chat {
        Chat::new(
            "chat-1",
            "user-1",
            "title",
            Visibility::Private,
            Utc.timestamp_opt(1, 0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn dto_uses_explicit_wire_fields_and_round_trips_domain_state() {
        let document = ChatDocument::from_create_chat(&chat()).unwrap();
        assert_eq!(document.user_id, "user-1");
        assert_eq!(document.lifecycle, "active");
        assert_eq!(document.lifecycle_revision, 0);
        assert_eq!(document.into_chat("chat-1").unwrap(), chat());
    }

    #[test]
    fn malformed_identity_and_tombstone_data_are_rejected() {
        let mut document = ChatDocument::from_create_chat(&chat()).unwrap();
        document.id = "different".to_string();
        assert!(matches!(
            document.into_chat("chat-1"),
            Err(PersistenceError::CorruptData(_))
        ));

        let mut document = ChatDocument::from_create_chat(&chat()).unwrap();
        document.lifecycle = "deleted".to_string();
        assert!(matches!(
            document.into_chat("chat-1"),
            Err(PersistenceError::CorruptData(_))
        ));
    }

    #[test]
    fn known_commit_errors_do_not_become_ambiguous_or_trigger_reconciliation() {
        let error = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
            FirestoreErrorPublicGenericDetails::new("PermissionDenied".to_string()),
            "denied".to_string(),
            false,
        ));
        assert!(matches!(
            map_firestore_error(error, FirestoreOperation::UpdateCommit),
            PersistenceError::PermissionDenied(_)
        ));
    }

    #[test]
    fn ambiguous_commit_preserves_retryability_and_reconciliation_failure() {
        let error = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
            FirestoreErrorPublicGenericDetails::new("Unavailable".to_string()),
            "response lost".to_string(),
            true,
        ));
        let original = map_firestore_error(error, FirestoreOperation::DeleteCommit);
        assert!(matches!(
            original,
            PersistenceError::OutcomeUnknown {
                retryable: true,
                ..
            }
        ));
        let with_failure = original.with_reconciliation_failure(PersistenceError::CorruptData(
            "record cannot be decoded".to_string(),
        ));
        assert!(matches!(
            with_failure,
            PersistenceError::OutcomeUnknown {
                retryable: true,
                reconciliation: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn create_conflict_payload_comparison_is_not_an_ambiguous_recovery() {
        let first = ChatDocument::from_create_chat(&chat()).unwrap();
        let mut different = first.clone();
        different.user_id = "user-2".to_string();
        assert!(!different.has_same_create_payload(&chat()));
        assert!(!is_ambiguous_write(&PersistenceError::Conflict));
    }

    #[test]
    fn cursor_order_is_timestamp_then_id() {
        let first = PaginationPosition::new(Utc.timestamp_opt(1, 0).unwrap(), "a");
        let second = PaginationPosition::new(Utc.timestamp_opt(1, 0).unwrap(), "b");
        assert!(second < first || first < second);
    }
}
