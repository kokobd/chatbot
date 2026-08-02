use async_trait::async_trait;
use chrono::{DateTime, Utc};
use firestore::errors::FirestoreError;
use firestore::timestamp_utils::from_timestamp;
use firestore::{
    FirestoreConsistencySelector, FirestoreDb, FirestoreGetByIdSupport, FirestoreQueryCursor,
    FirestoreQueryDirection, FirestoreTransactionOps, FirestoreWritePrecondition,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use crate::application::repository::{MessageQuery, MessageRepository, PersistenceError};
use crate::domain::{LifecycleState, Message, MessageRole, PaginationPosition};

use super::firestore_chat_repository::{ChatDocument, CHATS_COLLECTION};

pub const MESSAGES_COLLECTION: &str = "messages";
const MAX_TRANSACTION_WRITES: usize = 500;
const MAX_FIRESTORE_DOCUMENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct MessageDocument {
    pub(crate) id: String,
    #[serde(rename = "chatId")]
    pub(crate) chat_id: String,
    #[serde(rename = "userId")]
    pub(crate) user_id: String,
    pub(crate) role: String,
    pub(crate) parts: serde_json::Value,
    pub(crate) attachments: serde_json::Value,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MessagePartsUpdate {
    parts: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct UsageCount {
    #[serde(rename = "messageCount")]
    message_count: i64,
}

impl MessageDocument {
    fn from_message(message: &Message) -> Result<Self, PersistenceError> {
        let document = Self {
            id: message.id.clone(),
            chat_id: message.chat_id.clone(),
            user_id: message.user_id.clone(),
            role: role_wire(message.role),
            parts: message.parts.clone(),
            attachments: message.attachments.clone(),
            created_at: message.created_at,
        };
        let encoded = serde_json::to_vec(&document)
            .map_err(|error| PersistenceError::Serialization(error.to_string()))?;
        if encoded.len() > MAX_FIRESTORE_DOCUMENT_BYTES {
            return Err(PersistenceError::InvalidInput(format!(
                "message document is {} bytes; maximum is {} bytes",
                encoded.len(),
                MAX_FIRESTORE_DOCUMENT_BYTES
            )));
        }
        Ok(document)
    }

    fn into_message(self, document_id: &str) -> Result<Message, PersistenceError> {
        validate_document_id(document_id, "message")?;
        if self.id != document_id {
            return Err(PersistenceError::CorruptData(
                "message ID does not match the document ID".to_string(),
            ));
        }
        let role = MessageRole::parse(&self.role)
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        let message = Message::new(
            self.id,
            self.chat_id,
            self.user_id,
            role,
            self.parts,
            self.attachments,
            self.created_at,
        )
        .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        Self::from_message(&message).map_err(|error| match error {
            PersistenceError::InvalidInput(message) => PersistenceError::CorruptData(message),
            other => other,
        })?;
        Ok(message)
    }

    fn has_same_immutable_payload(&self, message: &Message) -> bool {
        self.id == message.id
            && self.chat_id == message.chat_id
            && self.user_id == message.user_id
            && self.role == role_wire(message.role)
            && self.attachments == message.attachments
            && self.created_at == message.created_at
    }
}

pub struct FirestoreMessageRepository {
    db: FirestoreDb,
}

impl FirestoreMessageRepository {
    pub fn new(db: FirestoreDb) -> Self {
        Self { db }
    }

    fn message_parent(&self, chat_id: &str) -> Result<String, PersistenceError> {
        self.db
            .parent_path(CHATS_COLLECTION, chat_id)
            .map(String::from)
            .map_err(|error| map_firestore_error(error, MessageOperation::Setup))
    }

    async fn raw_message(
        &self,
        chat_id: &str,
        message_id: &str,
        operation: MessageOperation,
    ) -> Result<Option<(MessageDocument, Option<DateTime<Utc>>)>, PersistenceError> {
        self.raw_message_from(&self.db, chat_id, message_id, operation)
            .await
    }

    async fn raw_message_from(
        &self,
        db: &FirestoreDb,
        chat_id: &str,
        message_id: &str,
        operation: MessageOperation,
    ) -> Result<Option<(MessageDocument, Option<DateTime<Utc>>)>, PersistenceError> {
        validate_document_id(chat_id, "chat")?;
        validate_document_id(message_id, "message")?;
        let parent = self.message_parent(chat_id)?;
        let document = match db
            .get_doc_at(parent.as_str(), MESSAGES_COLLECTION, message_id, None)
            .await
        {
            Ok(document) => document,
            Err(error) => {
                let mapped = map_firestore_error(error, operation);
                if mapped == PersistenceError::NotFound {
                    return Ok(None);
                }
                return Err(mapped);
            }
        };
        let update_time = document
            .update_time
            .clone()
            .map(from_timestamp)
            .transpose()
            .map_err(|error| map_firestore_error(error, operation))?;
        let object = FirestoreDb::deserialize_doc_to::<MessageDocument>(&document)
            .map_err(|error| map_firestore_error(error, operation))?;
        Ok(Some((object, update_time)))
    }

    async fn active_chat_in(
        &self,
        db: &FirestoreDb,
        user_id: &str,
        chat_id: &str,
        operation: MessageOperation,
    ) -> Result<(), PersistenceError> {
        validate_identifier(user_id)?;
        validate_document_id(chat_id, "chat")?;
        let document: Option<ChatDocument> = db
            .fluent()
            .select()
            .by_id_in(CHATS_COLLECTION)
            .obj()
            .one(chat_id)
            .await
            .map_err(|error| map_firestore_error(error, operation))?;
        let Some(document) = document else {
            return Err(PersistenceError::NotFound);
        };
        let chat = document.into_chat(chat_id)?;
        if chat.user_id != user_id {
            return Err(PersistenceError::NotFound);
        }
        if chat.lifecycle == LifecycleState::Deleted {
            return Err(PersistenceError::FailedPrecondition(
                "chat is marked for deletion".to_string(),
            ));
        }
        Ok(())
    }

    async fn active_chat(&self, user_id: &str, chat_id: &str) -> Result<(), PersistenceError> {
        self.active_chat_in(&self.db, user_id, chat_id, MessageOperation::Read)
            .await
    }

    async fn execute_create_batch(&self, batch: &[Message]) -> Result<Vec<Message>, BatchFailure> {
        let mut transaction = self.db.begin_transaction().await.map_err(|error| {
            BatchFailure::Known(map_firestore_error(error, MessageOperation::Setup))
        })?;
        let transaction_db =
            self.db
                .clone_with_consistency_selector(FirestoreConsistencySelector::Transaction(
                    transaction.transaction_id().clone(),
                ));
        let mut owners = HashMap::new();
        for message in batch {
            owners
                .entry(message.chat_id.clone())
                .or_insert_with(|| message.user_id.clone());
        }
        for (chat_id, user_id) in owners {
            if let Err(error) = self
                .active_chat_in(&transaction_db, &user_id, &chat_id, MessageOperation::Read)
                .await
            {
                transaction.rollback().await.ok();
                return Err(BatchFailure::Known(error));
            }
        }

        let mut stored = Vec::with_capacity(batch.len());
        let mut writes = 0usize;
        for message in batch {
            let parent = self
                .message_parent(&message.chat_id)
                .map_err(BatchFailure::Known)?;
            let existing = self
                .read_message_from(&transaction_db, &parent, &message.id)
                .await
                .map_err(BatchFailure::Known)?;
            if let Some(existing) = existing {
                if existing != *message {
                    transaction.rollback().await.ok();
                    return Err(BatchFailure::Known(PersistenceError::Conflict));
                }
                stored.push(existing);
                continue;
            }

            let document = MessageDocument::from_message(message).map_err(BatchFailure::Known)?;
            transaction
                .update_object_at(
                    parent.as_str(),
                    MESSAGES_COLLECTION,
                    &message.id,
                    &document,
                    None,
                    Some(FirestoreWritePrecondition::Exists(false)),
                    vec![],
                )
                .map_err(|error| {
                    BatchFailure::Known(map_firestore_error(error, MessageOperation::Setup))
                })?;
            writes += 1;
            stored.push(message.clone());
        }

        if writes == 0 {
            transaction.rollback().await.ok();
            return Ok(stored);
        }
        let response = transaction.commit().await.map_err(|error| {
            let mapped = map_firestore_error(error, MessageOperation::Commit);
            if is_ambiguous_write(&mapped) {
                BatchFailure::Unknown(mapped)
            } else {
                BatchFailure::Known(mapped)
            }
        })?;
        if response.write_results.len() != writes {
            return Err(BatchFailure::Unknown(unknown_outcome(
                "message batch committed with an incomplete response",
                false,
            )));
        }
        Ok(stored)
    }

    async fn read_message_from(
        &self,
        db: &FirestoreDb,
        parent: &str,
        message_id: &str,
    ) -> Result<Option<Message>, PersistenceError> {
        let document = match db
            .get_doc_at(parent, MESSAGES_COLLECTION, message_id, None)
            .await
        {
            Ok(document) => document,
            Err(FirestoreError::DataNotFoundError(_)) => return Ok(None),
            Err(error) => {
                return Err(map_firestore_error(error, MessageOperation::Read));
            }
        };
        let object = FirestoreDb::deserialize_doc_to::<MessageDocument>(&document)
            .map_err(|error| map_firestore_error(error, MessageOperation::Read))?;
        object.into_message(message_id).map(Some)
    }

    async fn reconcile_batch(
        &self,
        batch: &[Message],
        original: PersistenceError,
    ) -> Result<Vec<Message>, PersistenceError> {
        let mut result = Vec::with_capacity(batch.len());
        for message in batch {
            match self
                .raw_message(&message.chat_id, &message.id, MessageOperation::Reconcile)
                .await
            {
                Ok(Some((document, _))) => {
                    let stored = match document.into_message(&message.id) {
                        Ok(stored) => stored,
                        Err(error) => return Err(original.with_reconciliation_failure(error)),
                    };
                    if stored != *message {
                        return Err(original);
                    }
                    result.push(stored);
                }
                Ok(None) => return Err(original),
                Err(error) => return Err(original.with_reconciliation_failure(error)),
            }
        }
        Ok(result)
    }

    async fn save_batch(&self, batch: &[Message]) -> Result<Vec<Message>, PersistenceError> {
        for attempt in 0..3 {
            match self.execute_create_batch(batch).await {
                Ok(messages) => return Ok(messages),
                Err(BatchFailure::Known(error))
                    if attempt < 2 && is_retryable_setup_or_contention(&error) =>
                {
                    continue;
                }
                Err(BatchFailure::Known(error)) => return Err(error),
                Err(BatchFailure::Unknown(error)) => {
                    return self.reconcile_batch(batch, error).await
                }
            }
        }
        unreachable!("batch retry loop always returns")
    }

    async fn update_message_inner(&self, message: &Message) -> Result<Message, PersistenceError> {
        let intended = MessageDocument::from_message(message)?;
        let mut transaction = self
            .db
            .begin_transaction()
            .await
            .map_err(|error| map_firestore_error(error, MessageOperation::Setup))?;
        let transaction_db =
            self.db
                .clone_with_consistency_selector(FirestoreConsistencySelector::Transaction(
                    transaction.transaction_id().clone(),
                ));
        if let Err(error) = self
            .active_chat_in(
                &transaction_db,
                &message.user_id,
                &message.chat_id,
                MessageOperation::Read,
            )
            .await
        {
            transaction.rollback().await.ok();
            return Err(error);
        }

        let parent = self.message_parent(&message.chat_id)?;
        let document = match transaction_db
            .get_doc_at(parent.as_str(), MESSAGES_COLLECTION, &message.id, None)
            .await
        {
            Ok(document) => document,
            Err(FirestoreError::DataNotFoundError(_)) => {
                transaction.rollback().await.ok();
                return Err(PersistenceError::NotFound);
            }
            Err(error) => {
                transaction.rollback().await.ok();
                return Err(map_firestore_error(error, MessageOperation::Read));
            }
        };
        let update_time = document
            .update_time
            .clone()
            .map(from_timestamp)
            .transpose()
            .map_err(|error| map_firestore_error(error, MessageOperation::Read))?
            .ok_or_else(|| {
                PersistenceError::CorruptData("message update time is missing".to_string())
            })?;
        let existing = FirestoreDb::deserialize_doc_to::<MessageDocument>(&document)
            .map_err(|error| map_firestore_error(error, MessageOperation::Read))?;
        let existing_message = existing.clone().into_message(&message.id)?;
        if !existing.has_same_immutable_payload(message) {
            transaction.rollback().await.ok();
            return Err(PersistenceError::Conflict);
        }
        if existing_message == *message {
            transaction.rollback().await.ok();
            return Ok(existing_message);
        }

        transaction
            .update_object_at(
                parent.as_str(),
                MESSAGES_COLLECTION,
                &message.id,
                &MessagePartsUpdate {
                    parts: intended.parts,
                },
                Some(vec!["parts".to_string()]),
                Some(FirestoreWritePrecondition::UpdateTime(update_time)),
                vec![],
            )
            .map_err(|error| map_firestore_error(error, MessageOperation::Setup))?;
        let response = transaction.commit().await.map_err(|error| {
            let mapped = map_firestore_error(error, MessageOperation::Commit);
            if is_ambiguous_write(&mapped) {
                BatchFailure::Unknown(mapped)
            } else {
                BatchFailure::Known(mapped)
            }
        });
        match response {
            Ok(response) if response.write_results.len() == 1 => Ok(message.clone()),
            Ok(_) => {
                self.reconcile_update(
                    message,
                    unknown_outcome(
                        "message update committed with an incomplete response",
                        false,
                    ),
                )
                .await
            }
            Err(BatchFailure::Known(error)) => Err(error),
            Err(BatchFailure::Unknown(error)) => self.reconcile_update(message, error).await,
        }
    }

    async fn reconcile_update(
        &self,
        message: &Message,
        original: PersistenceError,
    ) -> Result<Message, PersistenceError> {
        match self
            .raw_message(&message.chat_id, &message.id, MessageOperation::Reconcile)
            .await
        {
            Ok(Some((document, _))) => {
                let stored = match document.into_message(&message.id) {
                    Ok(stored) => stored,
                    Err(error) => return Err(original.with_reconciliation_failure(error)),
                };
                if stored == *message {
                    Ok(stored)
                } else {
                    Err(original)
                }
            }
            Ok(None) => Err(original),
            Err(error) => Err(original.with_reconciliation_failure(error)),
        }
    }

    async fn delete_messages_from_inner(
        &self,
        user_id: &str,
        chat_id: &str,
        position: &PaginationPosition,
    ) -> Result<u64, PersistenceError> {
        validate_identifier(user_id)?;
        validate_document_id(chat_id, "chat")?;
        validate_document_id(&position.id, "message")?;

        let mut transaction = self
            .db
            .begin_transaction()
            .await
            .map_err(|error| map_firestore_error(error, MessageOperation::Setup))?;
        let transaction_db =
            self.db
                .clone_with_consistency_selector(FirestoreConsistencySelector::Transaction(
                    transaction.transaction_id().clone(),
                ));

        if let Err(error) = self
            .active_chat_in(&transaction_db, user_id, chat_id, MessageOperation::Read)
            .await
        {
            transaction.rollback().await.ok();
            return Err(error);
        }

        let anchor = match self
            .raw_message_from(
                &transaction_db,
                chat_id,
                &position.id,
                MessageOperation::Read,
            )
            .await
        {
            Ok(Some((document, _))) => match document.into_message(&position.id) {
                Ok(message) => message,
                Err(error) => {
                    transaction.rollback().await.ok();
                    return Err(error);
                }
            },
            Ok(None) => {
                transaction.rollback().await.ok();
                return Err(PersistenceError::NotFound);
            }
            Err(error) => {
                transaction.rollback().await.ok();
                return Err(error);
            }
        };
        if anchor.chat_id != chat_id {
            transaction.rollback().await.ok();
            return Err(PersistenceError::CorruptData(
                "message ownership binding does not match its chat".to_string(),
            ));
        }
        if anchor.user_id != user_id {
            transaction.rollback().await.ok();
            return Err(PersistenceError::NotFound);
        }
        if anchor.created_at != position.created_at {
            transaction.rollback().await.ok();
            return Err(PersistenceError::FailedPrecondition(
                "message position is stale".to_string(),
            ));
        }

        let parent = self.message_parent(chat_id)?;
        let documents = transaction_db
            .fluent()
            .select()
            .from(MESSAGES_COLLECTION)
            .parent(parent.as_str())
            .order_by([
                ("createdAt", FirestoreQueryDirection::Ascending),
                ("id", FirestoreQueryDirection::Ascending),
            ])
            .start_at(FirestoreQueryCursor::BeforeValue(vec![
                position.created_at.into(),
                position.id.clone().into(),
            ]))
            .limit((MAX_TRANSACTION_WRITES + 1) as u32)
            .obj::<MessageDocument>()
            .query()
            .await
            .map_err(|error| map_firestore_error(error, MessageOperation::Read));
        let documents = match documents {
            Ok(documents) => documents,
            Err(error) => {
                transaction.rollback().await.ok();
                return Err(error);
            }
        };

        if documents.len() > MAX_TRANSACTION_WRITES {
            transaction.rollback().await.ok();
            return Err(PersistenceError::FailedPrecondition(
                "message branch exceeds transaction write limit".to_string(),
            ));
        }

        let mut candidates = Vec::with_capacity(documents.len());
        for document in documents {
            let document_id = document.id.clone();
            let message = match document.into_message(&document_id) {
                Ok(message) => message,
                Err(error) => {
                    transaction.rollback().await.ok();
                    return Err(error);
                }
            };
            if message.chat_id != chat_id || message.user_id != user_id {
                transaction.rollback().await.ok();
                return Err(PersistenceError::CorruptData(
                    "message ownership binding does not match its chat".to_string(),
                ));
            }
            candidates.push(message);
        }

        if candidates.is_empty() {
            transaction.rollback().await.ok();
            return Ok(0);
        }

        for message in &candidates {
            if let Err(error) = transaction
                .delete_by_id_at(parent.as_str(), MESSAGES_COLLECTION, &message.id, None)
                .map_err(|error| map_firestore_error(error, MessageOperation::Setup))
            {
                transaction.rollback().await.ok();
                return Err(error);
            }
        }

        let expected_count = candidates.len() as u64;
        let response = transaction.commit().await.map_err(|error| {
            let mapped = map_firestore_error(error, MessageOperation::Commit);
            if is_ambiguous_write(&mapped) {
                BatchFailure::Unknown(mapped)
            } else {
                BatchFailure::Known(mapped)
            }
        });
        match response {
            Ok(response) if response.write_results.len() == candidates.len() => Ok(expected_count),
            Ok(_) => {
                self.reconcile_deleted_messages(
                    &candidates,
                    unknown_outcome(
                        "message deletion committed with an incomplete response",
                        false,
                    ),
                )
                .await
            }
            Err(BatchFailure::Known(error)) => Err(error),
            Err(BatchFailure::Unknown(error)) => {
                self.reconcile_deleted_messages(&candidates, error).await
            }
        }
    }

    async fn reconcile_deleted_messages(
        &self,
        candidates: &[Message],
        original: PersistenceError,
    ) -> Result<u64, PersistenceError> {
        for message in candidates {
            match self
                .raw_message(&message.chat_id, &message.id, MessageOperation::Reconcile)
                .await
            {
                Ok(None) => {}
                Ok(Some(_)) => return Err(original),
                Err(error) => return Err(original.with_reconciliation_failure(error)),
            }
        }
        Ok(candidates.len() as u64)
    }
}

#[derive(Debug)]
enum BatchFailure {
    Known(PersistenceError),
    Unknown(PersistenceError),
}

#[async_trait]
impl MessageRepository for FirestoreMessageRepository {
    async fn save_messages(&self, messages: &[Message]) -> Result<Vec<Message>, PersistenceError> {
        let mut unique = Vec::with_capacity(messages.len());
        let mut by_id = HashMap::new();
        for message in messages {
            MessageDocument::from_message(message)?;
            if let Some(index) = by_id.get(&message.id) {
                if unique[*index] != *message {
                    return Err(PersistenceError::Conflict);
                }
                continue;
            }
            by_id.insert(message.id.clone(), unique.len());
            unique.push(message.clone());
        }

        let mut saved = Vec::with_capacity(unique.len());
        for batch in unique.chunks(MAX_TRANSACTION_WRITES) {
            saved.extend(self.save_batch(batch).await?);
        }
        let saved_by_id: HashMap<_, _> = saved
            .into_iter()
            .map(|message| (message.id.clone(), message))
            .collect();
        messages
            .iter()
            .map(|message| {
                saved_by_id
                    .get(&message.id)
                    .cloned()
                    .ok_or_else(|| PersistenceError::Internal {
                        message: "message batch progress could not be reconstructed".to_string(),
                        retryable: false,
                    })
            })
            .collect()
    }

    async fn update_message(&self, message: &Message) -> Result<Message, PersistenceError> {
        self.update_message_inner(message).await
    }

    async fn get_message_by_id(
        &self,
        user_id: &str,
        chat_id: &str,
        message_id: &str,
    ) -> Result<Option<Message>, PersistenceError> {
        self.active_chat(user_id, chat_id).await?;
        Ok(self
            .raw_message(chat_id, message_id, MessageOperation::Read)
            .await?
            .map(|(document, _)| document.into_message(message_id))
            .transpose()?
            .filter(|message| message.user_id == user_id))
    }

    async fn get_messages_by_chat_id(
        &self,
        query: &MessageQuery,
    ) -> Result<Vec<Message>, PersistenceError> {
        self.active_chat(&query.user_id, &query.chat_id).await?;
        let parent = self.message_parent(&query.chat_id)?;
        let documents = self
            .db
            .fluent()
            .select()
            .from(MESSAGES_COLLECTION)
            .parent(parent.as_str())
            .order_by([("createdAt", FirestoreQueryDirection::Ascending)])
            .obj::<MessageDocument>()
            .query()
            .await
            .map_err(|error| map_firestore_error(error, MessageOperation::Read))?;
        let mut messages: Vec<_> = documents
            .into_iter()
            .map(|document| {
                let document_id = document.id.clone();
                let message = document.into_message(&document_id)?;
                if message.chat_id != query.chat_id || message.user_id != query.user_id {
                    return Err(PersistenceError::CorruptData(
                        "message ownership binding does not match its chat".to_string(),
                    ));
                }
                Ok(message)
            })
            .collect::<Result<_, _>>()?;
        messages.sort_by_key(|message: &Message| (message.created_at, message.id.clone()));
        Ok(messages)
    }

    async fn count_user_messages(
        &self,
        user_id: &str,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, PersistenceError> {
        validate_identifier(user_id)?;
        let started = Instant::now();
        let result: Result<Vec<UsageCount>, PersistenceError> = self
            .db
            .fluent()
            .select()
            .from(MESSAGES_COLLECTION)
            .all_descendants()
            .filter(|filter| {
                filter.for_all([
                    filter.field("userId").eq(user_id.to_string()),
                    filter.field("role").eq("user"),
                    filter.field("createdAt").greater_than_or_equal(cutoff),
                ])
            })
            .aggregate(|aggregation| {
                aggregation.fields([aggregation.field("messageCount").count()])
            })
            .obj()
            .query()
            .await
            .map_err(|error| map_firestore_error(error, MessageOperation::UsageRead));
        let result = result.and_then(|values| {
            let count = values.first().map(|value| value.message_count).unwrap_or(0);
            if count < 0 {
                return Err(PersistenceError::CorruptData(
                    "message usage count is negative".to_string(),
                ));
            }
            Ok(count as u64)
        });
        tracing::info!(
            operation = "message_usage_count",
            latency_ms = started.elapsed().as_millis() as u64,
            firestore_read_cost_units = 1u64,
            result = if result.is_ok() { "ok" } else { "error" },
        );
        result
    }

    async fn delete_messages_from(
        &self,
        user_id: &str,
        chat_id: &str,
        position: &PaginationPosition,
    ) -> Result<u64, PersistenceError> {
        for attempt in 0..3 {
            match self
                .delete_messages_from_inner(user_id, chat_id, position)
                .await
            {
                Err(error) if attempt < 2 && is_retryable_delete_error(&error) => continue,
                result => return result,
            }
        }
        unreachable!("delete retry loop always returns")
    }
}

fn role_wire(role: MessageRole) -> String {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
        MessageRole::Tool => "tool",
    }
    .to_string()
}

fn validate_identifier(value: &str) -> Result<(), PersistenceError> {
    crate::domain::validate_identifier(value)
        .map(|_| ())
        .map_err(PersistenceError::from)
}

fn validate_document_id(value: &str, kind: &'static str) -> Result<(), PersistenceError> {
    validate_identifier(value)?;
    if value.contains('/') {
        return Err(PersistenceError::InvalidInput(format!(
            "{kind} ID must not contain '/'"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum MessageOperation {
    Read,
    Setup,
    Commit,
    UsageRead,
    Reconcile,
}

fn map_firestore_error(error: FirestoreError, operation: MessageOperation) -> PersistenceError {
    match error {
        FirestoreError::DataConflictError(_) => match operation {
            MessageOperation::Commit => PersistenceError::FailedPrecondition(
                "Firestore write precondition failed".to_string(),
            ),
            _ => PersistenceError::Conflict,
        },
        FirestoreError::DataNotFoundError(_) => PersistenceError::NotFound,
        FirestoreError::SerializeError(error) | FirestoreError::DeserializeError(error) => {
            match operation {
                MessageOperation::Commit => unknown_outcome(error.to_string(), false),
                MessageOperation::Read
                | MessageOperation::UsageRead
                | MessageOperation::Reconcile => PersistenceError::CorruptData(error.to_string()),
                MessageOperation::Setup => PersistenceError::Serialization(error.to_string()),
            }
        }
        FirestoreError::InvalidParametersError(error) => {
            PersistenceError::InvalidInput(error.to_string())
        }
        FirestoreError::DatabaseError(error) => {
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
        FirestoreError::NetworkError(error) => {
            map_unknown_or_unavailable(error.to_string(), operation, true)
        }
        FirestoreError::ErrorInTransaction(error) => {
            map_unknown_or_unavailable(error.to_string(), operation, false)
        }
        FirestoreError::SystemError(error) => match operation {
            MessageOperation::Commit => unknown_outcome(error.to_string(), false),
            _ => PersistenceError::Internal {
                message: error.to_string(),
                retryable: false,
            },
        },
        FirestoreError::CacheError(error) => PersistenceError::Internal {
            message: error.to_string(),
            retryable: false,
        },
    }
}

fn map_unknown_or_unavailable(
    message: String,
    operation: MessageOperation,
    retryable: bool,
) -> PersistenceError {
    match operation {
        MessageOperation::Commit => unknown_outcome(message, retryable),
        MessageOperation::Setup => PersistenceError::Unavailable { message, retryable },
        _ if retryable => PersistenceError::Unavailable { message, retryable },
        _ => PersistenceError::Internal { message, retryable },
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

fn is_retryable_setup_or_contention(error: &PersistenceError) -> bool {
    matches!(
        error,
        PersistenceError::Unavailable { .. } | PersistenceError::FailedPrecondition(_)
    )
}

fn is_retryable_delete_error(error: &PersistenceError) -> bool {
    match error {
        PersistenceError::Unavailable {
            retryable: true, ..
        } => true,
        PersistenceError::FailedPrecondition(message) => {
            message == "Firestore write precondition failed"
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::JsonValue;
    use chrono::TimeZone;
    use serde_json::json;

    fn message() -> Message {
        Message::new(
            "message-1",
            "chat-1",
            "user-1",
            MessageRole::User,
            json!([{ "text": "hello" }]),
            JsonValue::Array(vec![]),
            Utc.timestamp_opt(1, 0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn dto_round_trip_preserves_user_and_json_values() {
        let message = message();
        let document = MessageDocument::from_message(&message).unwrap();
        assert_eq!(document.into_message("message-1").unwrap(), message);
    }

    #[test]
    fn duplicate_comparison_includes_immutable_fields_and_excludes_parts() {
        let message = message();
        let document = MessageDocument::from_message(&message).unwrap();
        assert!(document.has_same_immutable_payload(&message));

        let mut changed = message.clone();
        changed.attachments = json!([{ "url": "different" }]);
        assert!(!document.has_same_immutable_payload(&changed));
    }

    #[test]
    fn malformed_identity_and_payload_limits_are_rejected() {
        let mut document = MessageDocument::from_message(&message()).unwrap();
        document.id = "other".to_string();
        assert!(matches!(
            document.into_message("message-1"),
            Err(PersistenceError::CorruptData(_))
        ));
        let oversized = Message::new(
            "message-2",
            "chat-1",
            "user-1",
            MessageRole::User,
            json!("x".repeat(crate::domain::validation::MAX_PAYLOAD_BYTES + 1)),
            JsonValue::Null,
            Utc.timestamp_opt(1, 0).unwrap(),
        );
        assert!(oversized.is_err());
    }

    #[test]
    fn total_encoded_document_size_is_checked_after_field_limits() {
        let message = Message::new(
            "message-3",
            "chat-1",
            "user-1",
            MessageRole::User,
            JsonValue::String("p".repeat(600_000)),
            JsonValue::String("a".repeat(500_000)),
            Utc.timestamp_opt(1, 0).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            MessageDocument::from_message(&message),
            Err(PersistenceError::InvalidInput(_))
        ));
    }

    #[test]
    fn permission_errors_are_not_ambiguous() {
        use firestore::errors::{FirestoreDatabaseError, FirestoreErrorPublicGenericDetails};
        let error = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
            FirestoreErrorPublicGenericDetails::new("PermissionDenied".to_string()),
            "denied".to_string(),
            false,
        ));
        assert!(matches!(
            map_firestore_error(error, MessageOperation::Commit),
            PersistenceError::PermissionDenied(_)
        ));
    }
}
