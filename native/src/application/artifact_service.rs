use std::sync::Arc;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::application::repository::{ArtifactRepository, PersistenceError};
use crate::domain::{Artifact, DocumentVersion, ValidationError};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArtifactServiceError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

pub struct ArtifactService {
    repository: Arc<dyn ArtifactRepository>,
}

impl ArtifactService {
    pub fn new(repository: Arc<dyn ArtifactRepository>) -> Self {
        Self { repository }
    }

    pub async fn create_artifact(
        &self,
        artifact: &Artifact,
    ) -> Result<Artifact, ArtifactServiceError> {
        Ok(self.repository.create_artifact(artifact).await?)
    }

    pub async fn find_artifact(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<Artifact>, ArtifactServiceError> {
        Ok(self.repository.find_artifact(user_id, artifact_id).await?)
    }

    pub async fn save_document_version(
        &self,
        user_id: &str,
        version: &DocumentVersion,
    ) -> Result<DocumentVersion, ArtifactServiceError> {
        Ok(self
            .repository
            .save_document_version(user_id, version)
            .await?)
    }

    pub async fn update_document_version(
        &self,
        user_id: &str,
        version: &DocumentVersion,
    ) -> Result<DocumentVersion, ArtifactServiceError> {
        Ok(self
            .repository
            .update_document_version(user_id, version)
            .await?)
    }

    pub async fn get_document_versions(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Vec<DocumentVersion>, ArtifactServiceError> {
        Ok(self
            .repository
            .get_document_versions(user_id, artifact_id)
            .await?)
    }

    pub async fn get_latest_document_version(
        &self,
        user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<DocumentVersion>, ArtifactServiceError> {
        Ok(self
            .repository
            .get_latest_document_version(user_id, artifact_id)
            .await?)
    }

    pub async fn delete_document_versions_after(
        &self,
        user_id: &str,
        artifact_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<DocumentVersion>, ArtifactServiceError> {
        Ok(self
            .repository
            .delete_document_versions_after(user_id, artifact_id, timestamp)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeArtifactRepository {
        artifacts: Mutex<HashMap<String, Artifact>>,
        versions: Mutex<HashMap<String, DocumentVersion>>,
    }

    #[async_trait]
    impl ArtifactRepository for FakeArtifactRepository {
        async fn create_artifact(&self, artifact: &Artifact) -> Result<Artifact, PersistenceError> {
            let mut artifacts = self.artifacts.lock().unwrap();
            match artifacts.get(&artifact.id) {
                Some(existing) if existing == artifact => Ok(existing.clone()),
                Some(_) => Err(PersistenceError::Conflict),
                None => {
                    artifacts.insert(artifact.id.clone(), artifact.clone());
                    Ok(artifact.clone())
                }
            }
        }

        async fn find_artifact(
            &self,
            user_id: &str,
            artifact_id: &str,
        ) -> Result<Option<Artifact>, PersistenceError> {
            Ok(self
                .artifacts
                .lock()
                .unwrap()
                .get(artifact_id)
                .filter(|artifact| artifact.user_id == user_id)
                .cloned())
        }

        async fn save_document_version(
            &self,
            user_id: &str,
            version: &DocumentVersion,
        ) -> Result<DocumentVersion, PersistenceError> {
            if self
                .artifacts
                .lock()
                .unwrap()
                .get(&version.document_id)
                .filter(|artifact| artifact.user_id == user_id)
                .is_none()
            {
                return Err(PersistenceError::NotFound);
            }
            let mut versions = self.versions.lock().unwrap();
            match versions.get(&version.version_id) {
                Some(existing) if existing == version => Ok(existing.clone()),
                Some(_) => Err(PersistenceError::Conflict),
                None => {
                    versions.insert(version.version_id.clone(), version.clone());
                    Ok(version.clone())
                }
            }
        }

        async fn get_document_versions(
            &self,
            user_id: &str,
            artifact_id: &str,
        ) -> Result<Vec<DocumentVersion>, PersistenceError> {
            if self.find_artifact(user_id, artifact_id).await?.is_none() {
                return Err(PersistenceError::NotFound);
            }
            let mut versions: Vec<_> = self
                .versions
                .lock()
                .unwrap()
                .values()
                .filter(|version| version.document_id == artifact_id)
                .cloned()
                .collect();
            versions.sort_by_key(|version| (version.created_at, version.version_id.clone()));
            Ok(versions)
        }

        async fn get_latest_document_version(
            &self,
            user_id: &str,
            artifact_id: &str,
        ) -> Result<Option<DocumentVersion>, PersistenceError> {
            Ok(self
                .get_document_versions(user_id, artifact_id)
                .await?
                .pop())
        }

        async fn delete_document_versions_after(
            &self,
            user_id: &str,
            artifact_id: &str,
            timestamp: DateTime<Utc>,
        ) -> Result<Vec<DocumentVersion>, PersistenceError> {
            if self.find_artifact(user_id, artifact_id).await?.is_none() {
                return Err(PersistenceError::NotFound);
            }
            let mut versions = self.versions.lock().unwrap();
            let removed = versions
                .values()
                .filter(|version| {
                    version.document_id == artifact_id && version.created_at > timestamp
                })
                .cloned()
                .collect();
            versions.retain(|_, version| {
                !(version.document_id == artifact_id && version.created_at > timestamp)
            });
            Ok(removed)
        }
    }

    fn artifact() -> Artifact {
        Artifact::new(
            "artifact-1",
            "user-1",
            "Document",
            crate::domain::ArtifactKind::Text,
            None,
            Utc.timestamp_opt(1, 0).unwrap(),
        )
        .unwrap()
    }

    fn version(id: &str, seconds: i64) -> DocumentVersion {
        DocumentVersion::new(
            id,
            "artifact-1",
            Utc.timestamp_opt(seconds, 0).unwrap(),
            Some(serde_json::json!({"version": id})),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn service_delegates_version_lifecycle_and_preserves_order() {
        let repository = Arc::new(FakeArtifactRepository::default());
        let service = ArtifactService::new(repository);
        service.create_artifact(&artifact()).await.unwrap();
        service
            .save_document_version("user-1", &version("second", 2))
            .await
            .unwrap();
        service
            .save_document_version("user-1", &version("first", 1))
            .await
            .unwrap();

        let versions = service
            .get_document_versions("user-1", "artifact-1")
            .await
            .unwrap();
        assert_eq!(versions[0].version_id, "first");
        assert_eq!(
            service
                .get_latest_document_version("user-1", "artifact-1")
                .await
                .unwrap()
                .unwrap()
                .version_id,
            "second"
        );
    }

    #[tokio::test]
    async fn service_returns_stable_conflicts_and_not_found_errors() {
        let service = ArtifactService::new(Arc::new(FakeArtifactRepository::default()));
        let value = artifact();
        service.create_artifact(&value).await.unwrap();
        assert_eq!(
            service
                .save_document_version("other-user", &version("version-1", 1),)
                .await,
            Err(ArtifactServiceError::Persistence(
                PersistenceError::NotFound
            ))
        );
        let mut conflicting = value.clone();
        conflicting.title = "Other title".to_string();
        assert_eq!(
            service.create_artifact(&conflicting).await,
            Err(ArtifactServiceError::Persistence(
                PersistenceError::Conflict
            ))
        );
    }
}
