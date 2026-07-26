use async_trait::async_trait;
use chrono::{DateTime, Utc};
use firestore::errors::FirestoreError;
use firestore::{
    FirestoreConsistencySelector, FirestoreDb, FirestoreGetByIdSupport, FirestoreQueryDirection,
    FirestoreTransactionOps, FirestoreWritePrecondition,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::application::repository::{ArtifactRepository, PersistenceError};
use crate::domain::{Artifact, ArtifactKind, DocumentVersion, Suggestion};

pub const ARTIFACTS_COLLECTION: &str = "artifacts";
pub const VERSIONS_COLLECTION: &str = "versions";
pub const SUGGESTIONS_COLLECTION: &str = "suggestions";
const MAX_TRANSACTION_WRITES: usize = 500;
const MAX_FIRESTORE_DOCUMENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ArtifactDocument {
    pub(crate) id: String,
    #[serde(rename = "userId")]
    pub(crate) user_id: String,
    pub(crate) title: String,
    pub(crate) kind: String,
    pub(crate) content: Option<serde_json::Value>,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: DateTime<Utc>,
    #[serde(rename = "headVersionId")]
    pub(crate) head_version_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct VersionDocument {
    #[serde(rename = "versionId")]
    pub(crate) version_id: String,
    #[serde(rename = "documentId")]
    pub(crate) document_id: String,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) content: Option<serde_json::Value>,
    #[serde(rename = "cleanupAt")]
    pub(crate) cleanup_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ArtifactHeadUpdate {
    #[serde(rename = "headVersionId")]
    head_version_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct VersionCleanupUpdate {
    #[serde(rename = "cleanupAt")]
    cleanup_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SuggestionDocument {
    pub(crate) id: String,
    #[serde(rename = "documentId")]
    pub(crate) document_id: String,
    #[serde(rename = "versionId")]
    pub(crate) version_id: String,
    #[serde(rename = "userId")]
    pub(crate) user_id: String,
    #[serde(rename = "originalText")]
    pub(crate) original_text: String,
    #[serde(rename = "suggestedText")]
    pub(crate) suggested_text: String,
    pub(crate) description: Option<String>,
    #[serde(rename = "isResolved")]
    pub(crate) is_resolved: bool,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: DateTime<Utc>,
    #[serde(rename = "cleanupAt")]
    pub(crate) cleanup_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SuggestionCleanupUpdate {
    #[serde(rename = "cleanupAt")]
    cleanup_at: DateTime<Utc>,
}

impl ArtifactDocument {
    fn from_artifact(artifact: &Artifact) -> Result<Self, PersistenceError> {
        validate_document_id(&artifact.id, "artifact")?;
        validate_identifier(&artifact.user_id)?;
        if artifact.head_version_id.is_some() {
            return Err(PersistenceError::InvalidInput(
                "artifact create payload must not contain a head version".to_string(),
            ));
        }
        let document = Self {
            id: artifact.id.clone(),
            user_id: artifact.user_id.clone(),
            title: artifact.title.clone(),
            kind: artifact_kind_wire(artifact.kind),
            content: artifact.content.clone(),
            created_at: artifact.created_at,
            head_version_id: None,
        };
        validate_document_size(&document, "artifact")?;
        Ok(document)
    }

    fn into_artifact(self, document_id: &str) -> Result<Artifact, PersistenceError> {
        validate_document_id(document_id, "artifact")
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        if self.id != document_id {
            return Err(PersistenceError::CorruptData(
                "artifact ID does not match the document ID".to_string(),
            ));
        }
        if let Some(head_version_id) = &self.head_version_id {
            validate_document_id(head_version_id, "version")
                .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        }
        let kind = ArtifactKind::parse(&self.kind)
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        Artifact::new(
            self.id,
            self.user_id,
            self.title,
            kind,
            self.content,
            self.created_at,
        )
        .map_err(|error| PersistenceError::CorruptData(error.to_string()))?
        .with_head_version_id(self.head_version_id)
        .map_err(|error| PersistenceError::CorruptData(error.to_string()))
    }

    fn has_same_create_payload(&self, artifact: &Artifact) -> bool {
        self.id == artifact.id
            && self.user_id == artifact.user_id
            && self.title == artifact.title
            && self.kind == artifact_kind_wire(artifact.kind)
            && self.content == artifact.content
            && self.created_at == artifact.created_at
    }
}

impl VersionDocument {
    fn from_version(version: &DocumentVersion) -> Result<Self, PersistenceError> {
        validate_document_id(&version.version_id, "version")?;
        validate_document_id(&version.document_id, "artifact")?;
        let document = Self {
            version_id: version.version_id.clone(),
            document_id: version.document_id.clone(),
            created_at: version.created_at,
            content: version.content.clone(),
            cleanup_at: None,
        };
        validate_document_size(&document, "document version")?;
        Ok(document)
    }

    fn into_version(
        self,
        document_id: &str,
        version_id: &str,
    ) -> Result<DocumentVersion, PersistenceError> {
        validate_document_id(document_id, "artifact")
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        validate_document_id(version_id, "version")
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        if self.document_id != document_id || self.version_id != version_id {
            return Err(PersistenceError::CorruptData(
                "version identity does not match its document path".to_string(),
            ));
        }
        DocumentVersion::new(
            self.version_id,
            self.document_id,
            self.created_at,
            self.content,
        )
        .map_err(|error| PersistenceError::CorruptData(error.to_string()))
    }

    fn has_same_immutable_payload(&self, version: &DocumentVersion) -> bool {
        self.version_id == version.version_id
            && self.document_id == version.document_id
            && self.created_at == version.created_at
            && self.content == version.content
    }
}

impl SuggestionDocument {
    fn from_suggestion(suggestion: &Suggestion) -> Result<Self, PersistenceError> {
        validate_document_id(&suggestion.id, "suggestion")?;
        validate_document_id(&suggestion.document_id, "artifact")?;
        validate_document_id(&suggestion.version_id, "version")?;
        validate_identifier(&suggestion.user_id)?;
        let document = Self {
            id: suggestion.id.clone(),
            document_id: suggestion.document_id.clone(),
            version_id: suggestion.version_id.clone(),
            user_id: suggestion.user_id.clone(),
            original_text: suggestion.original_text.clone(),
            suggested_text: suggestion.suggested_text.clone(),
            description: suggestion.description.clone(),
            is_resolved: suggestion.is_resolved,
            created_at: suggestion.created_at,
            cleanup_at: None,
        };
        validate_document_size(&document, "suggestion")?;
        Ok(document)
    }

    fn into_suggestion(
        self,
        document_id: &str,
        suggestion_id: &str,
    ) -> Result<Suggestion, PersistenceError> {
        validate_document_id(document_id, "artifact")
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        validate_document_id(suggestion_id, "suggestion")
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        if self.id != suggestion_id || self.document_id != document_id {
            return Err(PersistenceError::CorruptData(
                "suggestion identity does not match its document path".to_string(),
            ));
        }
        Suggestion::new(
            self.id,
            self.document_id,
            self.version_id,
            self.user_id,
            self.original_text,
            self.suggested_text,
            self.description,
            self.created_at,
        )
        .map(|suggestion| suggestion.with_resolved(self.is_resolved))
        .map_err(|error| PersistenceError::CorruptData(error.to_string()))
    }

    fn has_same_immutable_payload(&self, suggestion: &Suggestion) -> bool {
        self.id == suggestion.id
            && self.document_id == suggestion.document_id
            && self.version_id == suggestion.version_id
            && self.user_id == suggestion.user_id
            && self.original_text == suggestion.original_text
            && self.suggested_text == suggestion.suggested_text
            && self.description == suggestion.description
            && self.created_at == suggestion.created_at
    }
}

pub struct FirestoreArtifactRepository {
    db: FirestoreDb,
}

impl FirestoreArtifactRepository {
    pub fn new(db: FirestoreDb) -> Self {
        Self { db }
    }

    fn version_parent(&self, artifact_id: &str) -> Result<String, PersistenceError> {
        self.db
            .parent_path(ARTIFACTS_COLLECTION, artifact_id)
            .map(String::from)
            .map_err(|error| map_firestore_error(error, ArtifactOperation::Setup))
    }

    async fn raw_artifact(
        &self,
        artifact_id: &str,
        operation: ArtifactOperation,
    ) -> Result<Option<Artifact>, PersistenceError> {
        validate_document_id(artifact_id, "artifact")?;
        let document = match self
            .db
            .get_doc(ARTIFACTS_COLLECTION, artifact_id, None)
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
        let document_id = document_id_from_name(&document.name, "artifact")?;
        if document_id != artifact_id {
            return Err(PersistenceError::CorruptData(
                "artifact document path does not match the requested ID".to_string(),
            ));
        }
        let object = FirestoreDb::deserialize_doc_to::<ArtifactDocument>(&document)
            .map_err(|error| map_firestore_error(error, operation))?;
        object.into_artifact(artifact_id).map(Some)
    }

    async fn raw_version(
        &self,
        artifact_id: &str,
        version_id: &str,
        operation: ArtifactOperation,
    ) -> Result<Option<(VersionDocument, DocumentVersion)>, PersistenceError> {
        validate_document_id(artifact_id, "artifact")?;
        validate_document_id(version_id, "version")?;
        let parent = self.version_parent(artifact_id)?;
        let document = match self
            .db
            .get_doc_at(parent.as_str(), VERSIONS_COLLECTION, version_id, None)
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
        let object = FirestoreDb::deserialize_doc_to::<VersionDocument>(&document)
            .map_err(|error| map_firestore_error(error, operation))?;
        let version = object.clone().into_version(artifact_id, version_id)?;
        Ok(Some((object, version)))
    }

    async fn owned_artifact(
        &self,
        user_id: &str,
        artifact_id: &str,
        operation: ArtifactOperation,
    ) -> Result<Option<Artifact>, PersistenceError> {
        validate_identifier(user_id)?;
        Ok(self
            .raw_artifact(artifact_id, operation)
            .await?
            .filter(|artifact| artifact.user_id == user_id))
    }

    async fn read_artifact_in(
        db: &FirestoreDb,
        artifact_id: &str,
        operation: ArtifactOperation,
    ) -> Result<Option<ArtifactDocument>, PersistenceError> {
        let document: Option<ArtifactDocument> = db
            .fluent()
            .select()
            .by_id_in(ARTIFACTS_COLLECTION)
            .obj()
            .one(artifact_id)
            .await
            .map_err(|error| map_firestore_error(error, operation))?;
        Ok(document)
    }

    async fn read_version_in(
        db: &FirestoreDb,
        parent: &str,
        version_id: &str,
        operation: ArtifactOperation,
    ) -> Result<Option<VersionDocument>, PersistenceError> {
        let document = match db
            .get_doc_at(parent, VERSIONS_COLLECTION, version_id, None)
            .await
        {
            Ok(document) => document,
            Err(FirestoreError::DataNotFoundError(_)) => return Ok(None),
            Err(error) => return Err(map_firestore_error(error, operation)),
        };
        FirestoreDb::deserialize_doc_to::<VersionDocument>(&document)
            .map(Some)
            .map_err(|error| map_firestore_error(error, operation))
    }

    async fn read_suggestion_in(
        db: &FirestoreDb,
        parent: &str,
        suggestion_id: &str,
        operation: ArtifactOperation,
    ) -> Result<Option<SuggestionDocument>, PersistenceError> {
        let document = match db
            .get_doc_at(parent, SUGGESTIONS_COLLECTION, suggestion_id, None)
            .await
        {
            Ok(document) => document,
            Err(FirestoreError::DataNotFoundError(_)) => return Ok(None),
            Err(error) => return Err(map_firestore_error(error, operation)),
        };
        FirestoreDb::deserialize_doc_to::<SuggestionDocument>(&document)
            .map(Some)
            .map_err(|error| map_firestore_error(error, operation))
    }

    async fn read_suggestions_in(
        db: &FirestoreDb,
        parent: &str,
        operation: ArtifactOperation,
    ) -> Result<Vec<SuggestionDocument>, PersistenceError> {
        db.fluent()
            .select()
            .from(SUGGESTIONS_COLLECTION)
            .parent(parent)
            .obj()
            .query()
            .await
            .map_err(|error| map_firestore_error(error, operation))
    }

    async fn execute_save_suggestions(
        &self,
        user_id: &str,
        batch: &[Suggestion],
    ) -> Result<Vec<Suggestion>, BatchFailure> {
        let mut transaction = self.db.begin_transaction().await.map_err(|error| {
            BatchFailure::Known(map_firestore_error(
                error,
                ArtifactOperation::TransactionSetup,
            ))
        })?;
        let transaction_db =
            self.db
                .clone_with_consistency_selector(FirestoreConsistencySelector::Transaction(
                    transaction.transaction_id().clone(),
                ));
        let mut artifacts: HashMap<String, ArtifactDocument> = HashMap::new();
        let mut stored = Vec::with_capacity(batch.len());
        let mut writes = 0usize;

        for suggestion in batch {
            let parent = self
                .version_parent(&suggestion.document_id)
                .map_err(BatchFailure::Known)?;
            let artifact_document = if let Some(document) = artifacts.get(&suggestion.document_id) {
                document.clone()
            } else {
                let document = Self::read_artifact_in(
                    &transaction_db,
                    &suggestion.document_id,
                    ArtifactOperation::TransactionRead,
                )
                .await
                .map_err(BatchFailure::Known)?
                .ok_or_else(|| BatchFailure::Known(PersistenceError::NotFound))?;
                artifacts.insert(suggestion.document_id.clone(), document.clone());
                document
            };
            if artifact_document.user_id != user_id {
                transaction.rollback().await.ok();
                return Err(BatchFailure::Known(PersistenceError::NotFound));
            }

            let Some(version_document) = Self::read_version_in(
                &transaction_db,
                parent.as_str(),
                &suggestion.version_id,
                ArtifactOperation::TransactionRead,
            )
            .await
            .map_err(BatchFailure::Known)?
            else {
                transaction.rollback().await.ok();
                return Err(BatchFailure::Known(PersistenceError::NotFound));
            };
            version_document
                .clone()
                .into_version(&suggestion.document_id, &suggestion.version_id)
                .map_err(BatchFailure::Known)?;
            if version_document.cleanup_at.is_some()
                || version_document.document_id != suggestion.document_id
            {
                transaction.rollback().await.ok();
                return Err(BatchFailure::Known(PersistenceError::NotFound));
            }

            let existing = Self::read_suggestion_in(
                &transaction_db,
                parent.as_str(),
                &suggestion.id,
                ArtifactOperation::TransactionRead,
            )
            .await
            .map_err(BatchFailure::Known)?;
            if let Some(existing) = existing {
                let existing_suggestion = existing
                    .clone()
                    .into_suggestion(&suggestion.document_id, &suggestion.id)
                    .map_err(BatchFailure::Known)?;
                if existing.cleanup_at.is_some() || !existing.has_same_immutable_payload(suggestion)
                {
                    transaction.rollback().await.ok();
                    return Err(BatchFailure::Known(PersistenceError::Conflict));
                }
                stored.push(existing_suggestion);
                continue;
            }

            let intended =
                SuggestionDocument::from_suggestion(suggestion).map_err(BatchFailure::Known)?;
            transaction
                .update_object_at(
                    parent.as_str(),
                    SUGGESTIONS_COLLECTION,
                    &suggestion.id,
                    &intended,
                    None,
                    Some(FirestoreWritePrecondition::Exists(false)),
                    vec![],
                )
                .map_err(|error| {
                    BatchFailure::Known(map_firestore_error(
                        error,
                        ArtifactOperation::TransactionSetup,
                    ))
                })?;
            writes += 1;
            stored.push(suggestion.clone());
        }

        if writes == 0 {
            transaction.rollback().await.ok();
            return Ok(stored);
        }
        let response = transaction.commit().await.map_err(|error| {
            let mapped = map_firestore_error(error, ArtifactOperation::TransactionCommit);
            if is_ambiguous_write(&mapped) {
                BatchFailure::Unknown(mapped)
            } else {
                BatchFailure::Known(mapped)
            }
        })?;
        if response.write_results.len() != writes {
            return Err(BatchFailure::Unknown(unknown_outcome(
                "suggestion batch committed with an incomplete response",
                false,
            )));
        }
        Ok(stored)
    }

    async fn reconcile_suggestions(
        &self,
        batch: &[Suggestion],
        original: PersistenceError,
    ) -> Result<Vec<Suggestion>, PersistenceError> {
        let mut result = Vec::with_capacity(batch.len());
        for suggestion in batch {
            let parent = match self.version_parent(&suggestion.document_id) {
                Ok(parent) => parent,
                Err(error) => return Err(original.with_reconciliation_failure(error)),
            };
            let stored = match Self::read_suggestion_in(
                &self.db,
                parent.as_str(),
                &suggestion.id,
                ArtifactOperation::ReconcileRead,
            )
            .await
            {
                Ok(Some(document)) => {
                    if document.cleanup_at.is_some()
                        || !document.has_same_immutable_payload(suggestion)
                    {
                        return Err(original);
                    }
                    document
                        .into_suggestion(&suggestion.document_id, &suggestion.id)
                        .map_err(|error| original.clone().with_reconciliation_failure(error))?
                }
                Ok(None) => return Err(original),
                Err(error) => return Err(original.with_reconciliation_failure(error)),
            };
            result.push(stored);
        }
        Ok(result)
    }

    async fn execute_save_version(
        &self,
        user_id: &str,
        version: &DocumentVersion,
    ) -> Result<DocumentVersion, TransactionFailure> {
        let intended = VersionDocument::from_version(version).map_err(TransactionFailure::Known)?;
        let parent = self
            .version_parent(&version.document_id)
            .map_err(TransactionFailure::Known)?;
        let mut transaction = self.db.begin_transaction().await.map_err(|error| {
            TransactionFailure::Known(map_firestore_error(
                error,
                ArtifactOperation::TransactionSetup,
            ))
        })?;
        let transaction_db =
            self.db
                .clone_with_consistency_selector(FirestoreConsistencySelector::Transaction(
                    transaction.transaction_id().clone(),
                ));

        let Some(artifact_document) = Self::read_artifact_in(
            &transaction_db,
            &version.document_id,
            ArtifactOperation::TransactionRead,
        )
        .await
        .map_err(TransactionFailure::Known)?
        else {
            transaction.rollback().await.ok();
            return Err(TransactionFailure::Known(PersistenceError::NotFound));
        };
        let artifact = artifact_document
            .clone()
            .into_artifact(&version.document_id)
            .map_err(TransactionFailure::Known)?;
        if artifact.user_id != user_id {
            transaction.rollback().await.ok();
            return Err(TransactionFailure::Known(PersistenceError::NotFound));
        }

        let existing = Self::read_version_in(
            &transaction_db,
            parent.as_str(),
            &version.version_id,
            ArtifactOperation::TransactionRead,
        )
        .await
        .map_err(TransactionFailure::Known)?;
        if let Some(existing) = &existing {
            let existing_version = existing
                .clone()
                .into_version(&version.document_id, &version.version_id)
                .map_err(TransactionFailure::Known)?;
            if existing.cleanup_at.is_some() {
                transaction.rollback().await.ok();
                return Err(TransactionFailure::Known(PersistenceError::Conflict));
            }
            if !existing.has_same_immutable_payload(version) {
                transaction.rollback().await.ok();
                return Err(TransactionFailure::Known(PersistenceError::Conflict));
            }
            let current_head = current_head_version(
                &transaction_db,
                &parent,
                &artifact_document,
                &version.document_id,
                ArtifactOperation::TransactionRead,
            )
            .await
            .map_err(TransactionFailure::Known)?;
            if !should_advance_head(current_head.as_ref(), version) {
                transaction.rollback().await.ok();
                return Ok(existing_version);
            }
        }

        let current_head = current_head_version(
            &transaction_db,
            &parent,
            &artifact_document,
            &version.document_id,
            ArtifactOperation::TransactionRead,
        )
        .await
        .map_err(TransactionFailure::Known)?;
        let should_advance = should_advance_head(current_head.as_ref(), version);
        let mut writes = 0usize;
        if existing.is_none() {
            transaction
                .update_object_at(
                    parent.as_str(),
                    VERSIONS_COLLECTION,
                    &version.version_id,
                    &intended,
                    None,
                    Some(FirestoreWritePrecondition::Exists(false)),
                    vec![],
                )
                .map_err(|error| {
                    TransactionFailure::Known(map_firestore_error(
                        error,
                        ArtifactOperation::TransactionSetup,
                    ))
                })?;
            writes += 1;
        }
        if should_advance {
            transaction
                .update_object(
                    ARTIFACTS_COLLECTION,
                    &version.document_id,
                    &ArtifactHeadUpdate {
                        head_version_id: Some(version.version_id.clone()),
                    },
                    Some(vec!["headVersionId".to_string()]),
                    Some(FirestoreWritePrecondition::Exists(true)),
                    vec![],
                )
                .map_err(|error| {
                    TransactionFailure::Known(map_firestore_error(
                        error,
                        ArtifactOperation::TransactionSetup,
                    ))
                })?;
            writes += 1;
        }
        if writes == 0 {
            transaction.rollback().await.ok();
            return Ok(version.clone());
        }

        let response = transaction.commit().await.map_err(|error| {
            let mapped = map_firestore_error(error, ArtifactOperation::TransactionCommit);
            if is_ambiguous_write(&mapped) {
                TransactionFailure::Unknown(mapped)
            } else {
                TransactionFailure::Known(mapped)
            }
        })?;
        if response.write_results.len() != writes {
            return Err(TransactionFailure::Unknown(unknown_outcome(
                "artifact version transaction committed with an incomplete response",
                false,
            )));
        }
        Ok(version.clone())
    }

    async fn reconcile_saved_version(
        &self,
        user_id: &str,
        version: &DocumentVersion,
        original: PersistenceError,
    ) -> Result<DocumentVersion, PersistenceError> {
        let artifact = match self
            .owned_artifact(
                user_id,
                &version.document_id,
                ArtifactOperation::ReconcileRead,
            )
            .await
        {
            Ok(Some(artifact)) => artifact,
            Ok(None) => return Err(original),
            Err(error) => return Err(original.with_reconciliation_failure(error)),
        };
        let stored = match self
            .raw_version(
                &version.document_id,
                &version.version_id,
                ArtifactOperation::ReconcileRead,
            )
            .await
        {
            Ok(Some((document, stored))) => {
                if document.cleanup_at.is_some() || !document.has_same_immutable_payload(version) {
                    return Err(original);
                }
                stored
            }
            Ok(None) => return Err(original),
            Err(error) => return Err(original.with_reconciliation_failure(error)),
        };
        let Some(head_id) = artifact.head_version_id else {
            return Err(original);
        };
        let head = match self
            .raw_version(
                &version.document_id,
                &head_id,
                ArtifactOperation::ReconcileRead,
            )
            .await
        {
            Ok(Some((document, head))) if document.cleanup_at.is_none() => head,
            Ok(Some(_)) | Ok(None) => return Err(original),
            Err(error) => return Err(original.with_reconciliation_failure(error)),
        };
        if compare_versions(&head, &stored) >= std::cmp::Ordering::Equal {
            Ok(stored)
        } else {
            Err(original)
        }
    }

    async fn execute_delete_after(
        &self,
        user_id: &str,
        artifact_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<DocumentVersion>, TransactionFailure> {
        let parent = self
            .version_parent(artifact_id)
            .map_err(TransactionFailure::Known)?;
        let mut transaction = self.db.begin_transaction().await.map_err(|error| {
            TransactionFailure::Known(map_firestore_error(
                error,
                ArtifactOperation::TransactionSetup,
            ))
        })?;
        let transaction_db =
            self.db
                .clone_with_consistency_selector(FirestoreConsistencySelector::Transaction(
                    transaction.transaction_id().clone(),
                ));
        let Some(artifact_document) = Self::read_artifact_in(
            &transaction_db,
            artifact_id,
            ArtifactOperation::TransactionRead,
        )
        .await
        .map_err(TransactionFailure::Known)?
        else {
            transaction.rollback().await.ok();
            return Err(TransactionFailure::Known(PersistenceError::NotFound));
        };
        if artifact_document.user_id != user_id {
            transaction.rollback().await.ok();
            return Err(TransactionFailure::Known(PersistenceError::NotFound));
        }
        let documents: Vec<VersionDocument> = transaction_db
            .fluent()
            .select()
            .from(VERSIONS_COLLECTION)
            .parent(parent.as_str())
            .order_by([("createdAt", FirestoreQueryDirection::Ascending)])
            .obj()
            .query()
            .await
            .map_err(|error| {
                TransactionFailure::Known(map_firestore_error(
                    error,
                    ArtifactOperation::TransactionRead,
                ))
            })?;
        let mut versions = Vec::with_capacity(documents.len());
        for document in &documents {
            let version_id = document.version_id.clone();
            versions.push(
                document
                    .clone()
                    .into_version(artifact_id, &version_id)
                    .map_err(TransactionFailure::Known)?,
            );
        }
        let marked_ids: std::collections::HashSet<_> = documents
            .iter()
            .filter(|document| document.cleanup_at.is_none() && document.created_at > timestamp)
            .map(|document| document.version_id.as_str())
            .collect();
        let marked: Vec<_> = versions
            .iter()
            .filter(|version| marked_ids.contains(version.version_id.as_str()))
            .cloned()
            .collect();
        let cleanup_version_ids: HashSet<_> = documents
            .iter()
            .filter(|document| document.created_at > timestamp)
            .map(|document| document.version_id.as_str())
            .collect();
        let suggestion_documents = Self::read_suggestions_in(
            &transaction_db,
            parent.as_str(),
            ArtifactOperation::TransactionRead,
        )
        .await
        .map_err(TransactionFailure::Known)?;
        let suggestions_to_mark: Vec<_> = suggestion_documents
            .iter()
            .filter(|suggestion| {
                suggestion.cleanup_at.is_none()
                    && cleanup_version_ids.contains(suggestion.version_id.as_str())
            })
            .collect();
        if artifact_document.head_version_id.is_none() {
            if marked.is_empty() && suggestions_to_mark.is_empty() {
                transaction.rollback().await.ok();
                return Ok(Vec::new());
            }
            let mut writes = 0;
            for version in &marked {
                let update = VersionCleanupUpdate {
                    cleanup_at: Utc::now(),
                };
                transaction
                    .update_object_at(
                        parent.as_str(),
                        VERSIONS_COLLECTION,
                        &version.version_id,
                        &update,
                        Some(vec!["cleanupAt".to_string()]),
                        Some(FirestoreWritePrecondition::Exists(true)),
                        vec![],
                    )
                    .map_err(|error| {
                        TransactionFailure::Known(map_firestore_error(
                            error,
                            ArtifactOperation::TransactionSetup,
                        ))
                    })?;
                writes += 1;
            }
            for suggestion in &suggestions_to_mark {
                transaction
                    .update_object_at(
                        parent.as_str(),
                        SUGGESTIONS_COLLECTION,
                        &suggestion.id,
                        &SuggestionCleanupUpdate {
                            cleanup_at: Utc::now(),
                        },
                        Some(vec!["cleanupAt".to_string()]),
                        Some(FirestoreWritePrecondition::Exists(true)),
                        vec![],
                    )
                    .map_err(|error| {
                        TransactionFailure::Known(map_firestore_error(
                            error,
                            ArtifactOperation::TransactionSetup,
                        ))
                    })?;
                writes += 1;
            }
            commit_delete_transaction(transaction, writes, marked).await
        } else {
            let current_head = artifact_document
                .head_version_id
                .clone()
                .expect("head was checked above");
            let active: Vec<_> = versions
                .iter()
                .filter(|version| {
                    version.created_at <= timestamp
                        && documents.iter().any(|document| {
                            document.version_id == version.version_id
                                && document.cleanup_at.is_none()
                        })
                })
                .cloned()
                .collect();
            let next_head = active
                .iter()
                .max_by(|left, right| compare_versions(left, right));
            let head_is_marked = marked
                .iter()
                .any(|version| version.version_id == current_head);
            if marked.is_empty() && suggestions_to_mark.is_empty() {
                transaction.rollback().await.ok();
                return Ok(Vec::new());
            }
            let mut writes = 0;
            for version in &marked {
                let update = VersionCleanupUpdate {
                    cleanup_at: Utc::now(),
                };
                transaction
                    .update_object_at(
                        parent.as_str(),
                        VERSIONS_COLLECTION,
                        &version.version_id,
                        &update,
                        Some(vec!["cleanupAt".to_string()]),
                        Some(FirestoreWritePrecondition::Exists(true)),
                        vec![],
                    )
                    .map_err(|error| {
                        TransactionFailure::Known(map_firestore_error(
                            error,
                            ArtifactOperation::TransactionSetup,
                        ))
                    })?;
                writes += 1;
            }
            for suggestion in &suggestions_to_mark {
                transaction
                    .update_object_at(
                        parent.as_str(),
                        SUGGESTIONS_COLLECTION,
                        &suggestion.id,
                        &SuggestionCleanupUpdate {
                            cleanup_at: Utc::now(),
                        },
                        Some(vec!["cleanupAt".to_string()]),
                        Some(FirestoreWritePrecondition::Exists(true)),
                        vec![],
                    )
                    .map_err(|error| {
                        TransactionFailure::Known(map_firestore_error(
                            error,
                            ArtifactOperation::TransactionSetup,
                        ))
                    })?;
                writes += 1;
            }
            if head_is_marked {
                transaction
                    .update_object(
                        ARTIFACTS_COLLECTION,
                        artifact_id,
                        &ArtifactHeadUpdate {
                            head_version_id: next_head.map(|version| version.version_id.clone()),
                        },
                        Some(vec!["headVersionId".to_string()]),
                        Some(FirestoreWritePrecondition::Exists(true)),
                        vec![],
                    )
                    .map_err(|error| {
                        TransactionFailure::Known(map_firestore_error(
                            error,
                            ArtifactOperation::TransactionSetup,
                        ))
                    })?;
                writes += 1;
            }
            commit_delete_transaction(transaction, writes, marked).await
        }
    }
}

async fn commit_delete_transaction<'a>(
    transaction: firestore::FirestoreTransaction<'a>,
    writes: usize,
    marked: Vec<DocumentVersion>,
) -> Result<Vec<DocumentVersion>, TransactionFailure> {
    let response = transaction.commit().await.map_err(|error| {
        let mapped = map_firestore_error(error, ArtifactOperation::TransactionCommit);
        if is_ambiguous_write(&mapped) {
            TransactionFailure::Unknown(mapped)
        } else {
            TransactionFailure::Known(mapped)
        }
    })?;
    if response.write_results.len() != writes {
        return Err(TransactionFailure::Unknown(unknown_outcome(
            "artifact cleanup transaction committed with an incomplete response",
            false,
        )));
    }
    Ok(marked)
}

#[derive(Debug)]
enum TransactionFailure {
    Known(PersistenceError),
    Unknown(PersistenceError),
}

#[derive(Debug)]
enum BatchFailure {
    Known(PersistenceError),
    Unknown(PersistenceError),
}

#[async_trait]
impl ArtifactRepository for FirestoreArtifactRepository {
    async fn create_artifact(&self, artifact: &Artifact) -> Result<Artifact, PersistenceError> {
        let intended = ArtifactDocument::from_artifact(artifact)?;
        let document = FirestoreDb::serialize_to_doc("", &intended)
            .map_err(|error| map_firestore_error(error, ArtifactOperation::CreateSetup))?;
        match self
            .db
            .fluent()
            .insert()
            .into(ARTIFACTS_COLLECTION)
            .document_id(&artifact.id)
            .document(document)
            .execute()
            .await
        {
            Ok(_) => Ok(artifact.clone()),
            Err(error) => {
                let mapped = map_firestore_error(error, ArtifactOperation::CreateCommit);
                if mapped == PersistenceError::Conflict {
                    match self
                        .raw_artifact(&artifact.id, ArtifactOperation::ConflictRead)
                        .await
                    {
                        Ok(Some(stored)) if intended.has_same_create_payload(artifact) => {
                            if stored.user_id == artifact.user_id
                                && stored.title == artifact.title
                                && stored.kind == artifact.kind
                                && stored.content == artifact.content
                                && stored.created_at == artifact.created_at
                            {
                                Ok(stored)
                            } else {
                                Err(PersistenceError::Conflict)
                            }
                        }
                        Ok(Some(_)) | Ok(None) => Err(PersistenceError::Conflict),
                        Err(error) => Err(error),
                    }
                } else if is_ambiguous_write(&mapped) {
                    self.reconcile_create(artifact, mapped).await
                } else {
                    Err(mapped)
                }
            }
        }
    }

    async fn find_artifact(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<Artifact>, PersistenceError> {
        self.owned_artifact(user_id, artifact_id, ArtifactOperation::Read)
            .await
    }

    async fn save_document_version(
        &self,
        user_id: &str,
        version: &DocumentVersion,
    ) -> Result<DocumentVersion, PersistenceError> {
        validate_identifier(user_id)?;
        VersionDocument::from_version(version)?;
        for attempt in 0..3 {
            match self.execute_save_version(user_id, version).await {
                Ok(version) => return Ok(version),
                Err(TransactionFailure::Known(error))
                    if attempt < 2 && is_retryable_setup(&error) =>
                {
                    continue;
                }
                Err(TransactionFailure::Known(error)) => return Err(error),
                Err(TransactionFailure::Unknown(error)) => {
                    return self.reconcile_saved_version(user_id, version, error).await
                }
            }
        }
        unreachable!("version transaction retry loop always returns")
    }

    async fn get_document_versions(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Vec<DocumentVersion>, PersistenceError> {
        let Some(_) = self
            .owned_artifact(user_id, artifact_id, ArtifactOperation::Read)
            .await?
        else {
            return Err(PersistenceError::NotFound);
        };
        let parent = self.version_parent(artifact_id)?;
        let documents: Vec<VersionDocument> = self
            .db
            .fluent()
            .select()
            .from(VERSIONS_COLLECTION)
            .parent(parent.as_str())
            .order_by([("createdAt", FirestoreQueryDirection::Ascending)])
            .obj()
            .query()
            .await
            .map_err(|error| map_firestore_error(error, ArtifactOperation::VersionQuery))?;
        let mut versions = Vec::with_capacity(documents.len());
        for document in documents {
            if document.cleanup_at.is_some() {
                continue;
            }
            let version_id = document.version_id.clone();
            versions.push(document.into_version(artifact_id, &version_id)?);
        }
        versions.sort_by(|left, right| compare_versions(left, right));
        Ok(versions)
    }

    async fn get_latest_document_version(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<DocumentVersion>, PersistenceError> {
        let Some(artifact) = self
            .owned_artifact(user_id, artifact_id, ArtifactOperation::Read)
            .await?
        else {
            return Err(PersistenceError::NotFound);
        };
        let Some(head_version_id) = artifact.head_version_id else {
            return Ok(None);
        };
        let Some((document, version)) = self
            .raw_version(artifact_id, &head_version_id, ArtifactOperation::Read)
            .await?
        else {
            return Err(PersistenceError::CorruptData(
                "artifact head points to a missing version".to_string(),
            ));
        };
        if document.cleanup_at.is_some() {
            return Err(PersistenceError::CorruptData(
                "artifact head points to a version marked for cleanup".to_string(),
            ));
        }
        Ok(Some(version))
    }

    async fn delete_document_versions_after(
        &self,
        user_id: &str,
        artifact_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<DocumentVersion>, PersistenceError> {
        validate_identifier(user_id)?;
        validate_document_id(artifact_id, "artifact")?;
        for attempt in 0..3 {
            match self
                .execute_delete_after(user_id, artifact_id, timestamp)
                .await
            {
                Ok(versions) => return Ok(versions),
                Err(TransactionFailure::Known(error))
                    if attempt < 2 && is_retryable_setup(&error) =>
                {
                    continue;
                }
                Err(TransactionFailure::Known(error)) => return Err(error),
                Err(TransactionFailure::Unknown(error)) => {
                    return self
                        .reconcile_delete_after(user_id, artifact_id, timestamp, error)
                        .await
                }
            }
        }
        unreachable!("cleanup transaction retry loop always returns")
    }

    async fn save_suggestions(
        &self,
        user_id: &str,
        suggestions: &[Suggestion],
    ) -> Result<Vec<Suggestion>, PersistenceError> {
        validate_identifier(user_id)?;
        let mut unique = Vec::with_capacity(suggestions.len());
        let mut by_key = HashMap::new();
        for suggestion in suggestions {
            if suggestion.user_id != user_id {
                return Err(PersistenceError::NotFound);
            }
            SuggestionDocument::from_suggestion(suggestion)?;
            let key = (suggestion.document_id.clone(), suggestion.id.clone());
            if let Some(index) = by_key.get(&key) {
                if !SuggestionDocument::from_suggestion(&unique[*index])?
                    .has_same_immutable_payload(suggestion)
                {
                    return Err(PersistenceError::Conflict);
                }
                continue;
            }
            by_key.insert(key, unique.len());
            unique.push(suggestion.clone());
        }

        let mut saved = Vec::with_capacity(unique.len());
        for batch in unique.chunks(MAX_TRANSACTION_WRITES) {
            for attempt in 0..3 {
                match self.execute_save_suggestions(user_id, batch).await {
                    Ok(suggestions) => {
                        saved.extend(suggestions);
                        break;
                    }
                    Err(BatchFailure::Known(error))
                        if attempt < 2 && is_retryable_setup(&error) =>
                    {
                        continue;
                    }
                    Err(BatchFailure::Known(error)) => return Err(error),
                    Err(BatchFailure::Unknown(error)) => {
                        saved.extend(self.reconcile_suggestions(batch, error).await?);
                        break;
                    }
                }
            }
        }

        let saved_by_key: HashMap<_, _> = saved
            .into_iter()
            .map(|suggestion| {
                (
                    (suggestion.document_id.clone(), suggestion.id.clone()),
                    suggestion,
                )
            })
            .collect();
        suggestions
            .iter()
            .map(|suggestion| {
                saved_by_key
                    .get(&(suggestion.document_id.clone(), suggestion.id.clone()))
                    .cloned()
                    .ok_or_else(|| PersistenceError::Internal {
                        message: "suggestion batch progress could not be reconstructed".to_string(),
                        retryable: false,
                    })
            })
            .collect()
    }

    async fn get_suggestions_by_document_id(
        &self,
        user_id: &str,
        document_id: &str,
    ) -> Result<Vec<Suggestion>, PersistenceError> {
        let Some(_) = self
            .owned_artifact(user_id, document_id, ArtifactOperation::Read)
            .await?
        else {
            return Err(PersistenceError::NotFound);
        };
        let parent = self.version_parent(document_id)?;
        let documents = Self::read_suggestions_in(
            &self.db,
            parent.as_str(),
            ArtifactOperation::SuggestionQuery,
        )
        .await?;
        let mut suggestions = Vec::with_capacity(documents.len());
        for document in documents {
            if document.cleanup_at.is_some() {
                continue;
            }
            let suggestion_id = document.id.clone();
            let suggestion = document.into_suggestion(document_id, &suggestion_id)?;
            if suggestion.user_id != user_id {
                return Err(PersistenceError::CorruptData(
                    "suggestion ownership binding does not match its artifact".to_string(),
                ));
            }
            suggestions.push(suggestion);
        }
        suggestions.sort_by_key(|suggestion| (suggestion.created_at, suggestion.id.clone()));
        Ok(suggestions)
    }
}

impl FirestoreArtifactRepository {
    async fn reconcile_create(
        &self,
        artifact: &Artifact,
        original: PersistenceError,
    ) -> Result<Artifact, PersistenceError> {
        match self
            .raw_artifact(&artifact.id, ArtifactOperation::ReconcileRead)
            .await
        {
            Ok(Some(stored)) => {
                let intended = ArtifactDocument::from_artifact(artifact)
                    .map_err(|error| original.clone().with_reconciliation_failure(error))?;
                if intended.has_same_create_payload(artifact)
                    && stored.user_id == artifact.user_id
                    && stored.title == artifact.title
                    && stored.kind == artifact.kind
                    && stored.content == artifact.content
                    && stored.created_at == artifact.created_at
                {
                    Ok(stored)
                } else {
                    Err(original)
                }
            }
            Ok(None) => Err(original),
            Err(error) => Err(original.with_reconciliation_failure(error)),
        }
    }

    async fn reconcile_delete_after(
        &self,
        user_id: &str,
        artifact_id: &str,
        timestamp: DateTime<Utc>,
        original: PersistenceError,
    ) -> Result<Vec<DocumentVersion>, PersistenceError> {
        let artifact = match self
            .owned_artifact(user_id, artifact_id, ArtifactOperation::ReconcileRead)
            .await
        {
            Ok(Some(artifact)) => artifact,
            Ok(None) => return Err(original),
            Err(error) => return Err(original.with_reconciliation_failure(error)),
        };
        let versions = match self
            .get_all_versions(artifact_id, ArtifactOperation::ReconcileRead)
            .await
        {
            Ok(versions) => versions,
            Err(error) => return Err(original.with_reconciliation_failure(error)),
        };
        let intended: Vec<_> = versions
            .iter()
            .filter(|version| version.created_at > timestamp)
            .cloned()
            .collect();
        if intended.iter().any(|version| version.cleanup_at.is_none()) {
            return Err(original);
        }
        let active = versions
            .iter()
            .filter(|version| version.created_at <= timestamp && version.cleanup_at.is_none())
            .max_by(|left, right| compare_version_documents(left, right));
        if artifact.head_version_id.as_deref() != active.map(|version| version.version_id.as_str())
        {
            return Err(original);
        }
        let suggestions = match self
            .get_all_suggestions(artifact_id, ArtifactOperation::ReconcileRead)
            .await
        {
            Ok(suggestions) => suggestions,
            Err(error) => return Err(original.with_reconciliation_failure(error)),
        };
        if suggestions.iter().any(|suggestion| {
            intended
                .iter()
                .any(|version| version.version_id == suggestion.version_id)
                && suggestion.cleanup_at.is_none()
        }) {
            return Err(original);
        }
        intended
            .into_iter()
            .map(|version| {
                let version_id = version.version_id.clone();
                version.into_version(artifact_id, &version_id)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| original.with_reconciliation_failure(error))
    }

    async fn get_all_versions(
        &self,
        artifact_id: &str,
        operation: ArtifactOperation,
    ) -> Result<Vec<VersionDocument>, PersistenceError> {
        let parent = self.version_parent(artifact_id)?;
        self.db
            .fluent()
            .select()
            .from(VERSIONS_COLLECTION)
            .parent(parent.as_str())
            .order_by([("createdAt", FirestoreQueryDirection::Ascending)])
            .obj()
            .query()
            .await
            .map_err(|error| map_firestore_error(error, operation))
    }

    async fn get_all_suggestions(
        &self,
        artifact_id: &str,
        operation: ArtifactOperation,
    ) -> Result<Vec<SuggestionDocument>, PersistenceError> {
        let parent = self.version_parent(artifact_id)?;
        Self::read_suggestions_in(&self.db, parent.as_str(), operation).await
    }
}

async fn current_head_version(
    db: &FirestoreDb,
    parent: &str,
    artifact: &ArtifactDocument,
    artifact_id: &str,
    operation: ArtifactOperation,
) -> Result<Option<DocumentVersion>, PersistenceError> {
    let Some(head_id) = &artifact.head_version_id else {
        return Ok(None);
    };
    let Some(document) =
        FirestoreArtifactRepository::read_version_in(db, parent, head_id, operation).await?
    else {
        return Err(PersistenceError::CorruptData(
            "artifact head points to a missing version".to_string(),
        ));
    };
    if document.cleanup_at.is_some() {
        return Err(PersistenceError::CorruptData(
            "artifact head points to a version marked for cleanup".to_string(),
        ));
    }
    document.into_version(artifact_id, head_id).map(Some)
}

fn should_advance_head(current: Option<&DocumentVersion>, candidate: &DocumentVersion) -> bool {
    current
        .map(|current| compare_versions(candidate, current) == std::cmp::Ordering::Greater)
        .unwrap_or(true)
}

fn compare_versions(left: &DocumentVersion, right: &DocumentVersion) -> std::cmp::Ordering {
    (left.created_at, &left.version_id).cmp(&(right.created_at, &right.version_id))
}

fn compare_version_documents(
    left: &VersionDocument,
    right: &VersionDocument,
) -> std::cmp::Ordering {
    (left.created_at, &left.version_id).cmp(&(right.created_at, &right.version_id))
}

fn artifact_kind_wire(kind: ArtifactKind) -> String {
    serde_json::to_string(&kind)
        .expect("ArtifactKind serialization is infallible")
        .trim_matches('"')
        .to_string()
}

fn validate_document_size<T: Serialize>(document: &T, kind: &str) -> Result<(), PersistenceError> {
    let encoded = serde_json::to_vec(document)
        .map_err(|error| PersistenceError::Serialization(error.to_string()))?;
    if encoded.len() > MAX_FIRESTORE_DOCUMENT_BYTES {
        return Err(PersistenceError::InvalidInput(format!(
            "{kind} document is {} bytes; maximum is {} bytes",
            encoded.len(),
            MAX_FIRESTORE_DOCUMENT_BYTES
        )));
    }
    Ok(())
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

fn document_id_from_name<'a>(
    name: &'a str,
    kind: &'static str,
) -> Result<&'a str, PersistenceError> {
    name.rsplit('/')
        .next()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| PersistenceError::CorruptData(format!("{kind} document has no ID")))
}

#[derive(Debug, Clone, Copy)]
enum ArtifactOperation {
    Read,
    Setup,
    CreateSetup,
    CreateCommit,
    ConflictRead,
    TransactionSetup,
    TransactionRead,
    TransactionCommit,
    VersionQuery,
    SuggestionQuery,
    ReconcileRead,
}

fn map_firestore_error(error: FirestoreError, operation: ArtifactOperation) -> PersistenceError {
    match error {
        FirestoreError::DataConflictError(_) => match operation {
            ArtifactOperation::CreateCommit => PersistenceError::Conflict,
            ArtifactOperation::TransactionCommit => PersistenceError::FailedPrecondition(
                "Firestore transaction precondition failed".to_string(),
            ),
            _ => PersistenceError::Conflict,
        },
        FirestoreError::DataNotFoundError(_) => PersistenceError::NotFound,
        FirestoreError::SerializeError(error) => match operation {
            ArtifactOperation::CreateCommit | ArtifactOperation::TransactionCommit => {
                unknown_outcome(error.to_string(), false)
            }
            ArtifactOperation::Read
            | ArtifactOperation::ConflictRead
            | ArtifactOperation::TransactionRead
            | ArtifactOperation::VersionQuery
            | ArtifactOperation::SuggestionQuery
            | ArtifactOperation::ReconcileRead => PersistenceError::CorruptData(error.to_string()),
            _ => PersistenceError::Serialization(error.to_string()),
        },
        FirestoreError::DeserializeError(error) => match operation {
            ArtifactOperation::CreateCommit | ArtifactOperation::TransactionCommit => {
                unknown_outcome(error.to_string(), false)
            }
            ArtifactOperation::Read
            | ArtifactOperation::ConflictRead
            | ArtifactOperation::TransactionRead
            | ArtifactOperation::VersionQuery
            | ArtifactOperation::SuggestionQuery
            | ArtifactOperation::ReconcileRead => PersistenceError::CorruptData(error.to_string()),
            _ => PersistenceError::Serialization(error.to_string()),
        },
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
            ArtifactOperation::CreateCommit | ArtifactOperation::TransactionCommit => {
                unknown_outcome(error.to_string(), false)
            }
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
    operation: ArtifactOperation,
    retryable: bool,
) -> PersistenceError {
    match operation {
        ArtifactOperation::CreateCommit | ArtifactOperation::TransactionCommit => {
            unknown_outcome(message, retryable)
        }
        ArtifactOperation::CreateSetup
        | ArtifactOperation::Setup
        | ArtifactOperation::TransactionSetup => {
            PersistenceError::Unavailable { message, retryable }
        }
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

fn is_retryable_setup(error: &PersistenceError) -> bool {
    matches!(
        error,
        PersistenceError::Unavailable {
            retryable: true,
            ..
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use firestore::errors::{
        FirestoreDataConflictError, FirestoreDatabaseError, FirestoreErrorPublicGenericDetails,
        FirestoreSerializationError,
    };

    fn version(id: &str) -> DocumentVersion {
        DocumentVersion::new(id, "artifact-1", Utc.timestamp_opt(1, 0).unwrap(), None).unwrap()
    }

    #[test]
    fn version_identity_and_payload_are_explicit() {
        let document = VersionDocument::from_version(&version("version-1")).unwrap();
        assert_eq!(document.version_id, "version-1");
        assert_eq!(document.document_id, "artifact-1");
        assert!(document.has_same_immutable_payload(&version("version-1")));
    }

    fn suggestion(id: &str) -> Suggestion {
        Suggestion::new(
            id,
            "artifact-1",
            "version-1",
            "user-1",
            "original",
            "suggested",
            None,
            Utc.timestamp_opt(1, 0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn suggestion_dto_uses_explicit_wire_fields_and_preserves_resolution() {
        let suggestion = suggestion("suggestion-1").with_resolved(true);
        let document = SuggestionDocument::from_suggestion(&suggestion).unwrap();
        let encoded = serde_json::to_value(&document).unwrap();
        assert_eq!(encoded["documentId"], "artifact-1");
        assert_eq!(encoded["versionId"], "version-1");
        assert_eq!(encoded["userId"], "user-1");
        assert_eq!(encoded["originalText"], "original");
        assert_eq!(encoded["suggestedText"], "suggested");
        assert_eq!(encoded["isResolved"], true);
        assert_eq!(
            document
                .into_suggestion("artifact-1", "suggestion-1")
                .unwrap(),
            suggestion
        );
    }

    #[test]
    fn suggestion_duplicates_compare_immutable_fields_but_keep_resolution_mutable() {
        let left = suggestion("suggestion-1");
        let right = left.clone().with_resolved(true);
        let document = SuggestionDocument::from_suggestion(&left).unwrap();
        assert!(document.has_same_immutable_payload(&right));
    }

    #[test]
    fn equal_timestamps_use_version_id_for_head_ordering() {
        assert!(should_advance_head(Some(&version("a")), &version("b")));
        assert!(!should_advance_head(Some(&version("b")), &version("a")));
    }

    #[test]
    fn known_commit_failures_are_not_ambiguous() {
        let error = FirestoreError::DataConflictError(FirestoreDataConflictError::new(
            FirestoreErrorPublicGenericDetails::new("FailedPrecondition".to_string()),
            "stale transaction".to_string(),
        ));
        let mapped = map_firestore_error(error, ArtifactOperation::TransactionCommit);
        assert!(matches!(mapped, PersistenceError::FailedPrecondition(_)));
        assert!(!is_ambiguous_write(&mapped));
    }

    #[test]
    fn ambiguous_commit_and_post_commit_conversion_failures_reconcile() {
        let network = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
            FirestoreErrorPublicGenericDetails::new("Unavailable".to_string()),
            "response lost".to_string(),
            true,
        ));
        assert!(matches!(
            map_firestore_error(network, ArtifactOperation::TransactionCommit),
            PersistenceError::OutcomeUnknown {
                retryable: true,
                reconciliation: None,
                ..
            }
        ));
        let conversion = FirestoreError::DeserializeError(
            FirestoreSerializationError::from_message("bad response"),
        );
        assert!(matches!(
            map_firestore_error(conversion, ArtifactOperation::TransactionCommit),
            PersistenceError::OutcomeUnknown { .. }
        ));
    }

    #[test]
    fn missing_or_failed_reconciliation_keeps_the_original_unknown() {
        let original = unknown_outcome("response lost", true);
        assert_eq!(
            original
                .clone()
                .with_reconciliation_failure(PersistenceError::NotFound)
                .to_string()
                .contains("reconciliation failed"),
            true
        );
        assert!(matches!(
            original,
            PersistenceError::OutcomeUnknown {
                reconciliation: None,
                ..
            }
        ));
    }
}
