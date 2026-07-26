use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::domain::{Artifact, DocumentVersion};

use super::error::PersistenceError;

#[async_trait]
pub trait ArtifactRepository: Send + Sync {
    async fn create_artifact(&self, artifact: &Artifact) -> Result<Artifact, PersistenceError>;

    async fn find_artifact(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<Artifact>, PersistenceError>;

    async fn save_document_version(
        &self,
        user_id: &str,
        version: &DocumentVersion,
    ) -> Result<DocumentVersion, PersistenceError>;

    /// Saves a new immutable version and advances the artifact head when the
    /// supplied version is newer than the current head. Manual edits therefore
    /// allocate a new version ID instead of mutating an existing version.
    async fn update_document_version(
        &self,
        user_id: &str,
        version: &DocumentVersion,
    ) -> Result<DocumentVersion, PersistenceError> {
        self.save_document_version(user_id, version).await
    }

    async fn get_document_versions(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Vec<DocumentVersion>, PersistenceError>;

    async fn get_latest_document_version(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<DocumentVersion>, PersistenceError>;

    async fn delete_document_versions_after(
        &self,
        user_id: &str,
        artifact_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<DocumentVersion>, PersistenceError>;
}
