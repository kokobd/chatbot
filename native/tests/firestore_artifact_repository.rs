use std::env;
use std::fs;
use std::sync::Once;

use chatbot_native::domain::{Artifact, ArtifactKind, DocumentVersion, Suggestion};
use chatbot_native::{ArtifactRepository, FirestoreArtifactRepository, PersistenceError};
use chrono::{TimeZone, Utc};
use firestore::{FirestoreDb, FirestoreDbOptions};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ARTIFACTS_COLLECTION: &str = "artifacts";
const VERSIONS_COLLECTION: &str = "versions";
const SUGGESTIONS_COLLECTION: &str = "suggestions";
static LOAD_DOTENV: Once = Once::new();

fn load_dotenv() {
    LOAD_DOTENV.call_once(|| {
        let Ok(contents) = fs::read_to_string(".env") else {
            return;
        };
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            if env::var_os(name.trim()).is_none() {
                env::set_var(
                    name.trim(),
                    value.trim().trim_matches('"').trim_matches('\''),
                );
            }
        }
    });
}

async fn connect() -> FirestoreDb {
    load_dotenv();
    assert!(env::var_os("FIRESTORE_EMULATOR_HOST").is_none());
    let project_id = env::var("FIRESTORE_PROJECT_ID").expect("FIRESTORE_PROJECT_ID is required");
    let database_id = env::var("FIRESTORE_DATABASE_ID").expect("FIRESTORE_DATABASE_ID is required");
    assert_ne!(database_id, "(default)");
    let _ = rustls::crypto::ring::default_provider().install_default();
    FirestoreDb::with_options(FirestoreDbOptions::new(project_id).with_database_id(database_id))
        .await
        .unwrap()
}

fn id(label: &str) -> String {
    format!("plan-06-artifact-{label}-{}", Uuid::new_v4().simple())
}

fn artifact(id: &str, user_id: &str) -> Artifact {
    Artifact::new(
        id,
        user_id,
        "Artifact repository integration",
        ArtifactKind::Text,
        Some(serde_json::json!({"initial": true})),
        Utc.timestamp_opt(1, 0).unwrap(),
    )
    .unwrap()
}

fn version(version_id: &str, artifact_id: &str, seconds: i64) -> DocumentVersion {
    DocumentVersion::new(
        version_id,
        artifact_id,
        Utc.timestamp_opt(seconds, 0).unwrap(),
        Some(serde_json::json!({"version": version_id})),
    )
    .unwrap()
}

fn suggestion(
    suggestion_id: &str,
    artifact_id: &str,
    version_id: &str,
    user_id: &str,
) -> Suggestion {
    Suggestion::new(
        suggestion_id,
        artifact_id,
        version_id,
        user_id,
        "Original sentence",
        "Suggested sentence",
        Some("A clearer phrasing".to_string()),
        Utc.timestamp_opt(1, 0).unwrap(),
    )
    .unwrap()
}

async fn delete_version(db: &FirestoreDb, artifact_id: &str, version_id: &str) {
    let parent = db.parent_path(ARTIFACTS_COLLECTION, artifact_id).unwrap();
    let _ = db
        .fluent()
        .delete()
        .from(VERSIONS_COLLECTION)
        .parent(parent.as_ref())
        .document_id(version_id)
        .execute()
        .await;
}

async fn delete_suggestion(db: &FirestoreDb, artifact_id: &str, suggestion_id: &str) {
    let parent = db.parent_path(ARTIFACTS_COLLECTION, artifact_id).unwrap();
    let _ = db
        .fluent()
        .delete()
        .from(SUGGESTIONS_COLLECTION)
        .parent(parent.as_ref())
        .document_id(suggestion_id)
        .execute()
        .await;
}

async fn delete_artifact(db: &FirestoreDb, artifact_id: &str) {
    let _ = db
        .fluent()
        .delete()
        .from(ARTIFACTS_COLLECTION)
        .document_id(artifact_id)
        .execute()
        .await;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StoredVersionMarker {
    #[serde(rename = "versionId")]
    version_id: String,
    #[serde(rename = "documentId")]
    document_id: String,
    #[serde(rename = "createdAt")]
    created_at: chrono::DateTime<Utc>,
    content: Option<serde_json::Value>,
    #[serde(rename = "cleanupAt")]
    cleanup_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StoredSuggestionMarker {
    id: String,
    #[serde(rename = "documentId")]
    document_id: String,
    #[serde(rename = "versionId")]
    version_id: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "originalText")]
    original_text: String,
    #[serde(rename = "suggestedText")]
    suggested_text: String,
    description: Option<String>,
    #[serde(rename = "isResolved")]
    is_resolved: bool,
    #[serde(rename = "createdAt")]
    created_at: chrono::DateTime<Utc>,
    #[serde(rename = "cleanupAt")]
    cleanup_at: Option<chrono::DateTime<Utc>>,
}

#[tokio::test]
async fn versions_are_immutable_ordered_owned_and_logically_cleaned() {
    let db = connect().await;
    let first = FirestoreArtifactRepository::new(db.clone());
    let second = FirestoreArtifactRepository::new(db.clone());
    let user_id = id("user");
    let other_user_id = id("other-user");
    let artifact_id = id("document");
    let first_id = id("first");
    let second_id = id("second");
    let future_id = id("future");
    let artifact = artifact(&artifact_id, &user_id);
    let first_suggestion = suggestion(&id("suggestion-first"), &artifact_id, &first_id, &user_id);
    let future_suggestion =
        suggestion(&id("suggestion-future"), &artifact_id, &future_id, &user_id);

    let result: Result<(), PersistenceError> = async {
        assert_eq!(first.create_artifact(&artifact).await?, artifact);
        let duplicate = second.create_artifact(&artifact).await?;
        assert_eq!(duplicate.id, artifact_id);
        assert_eq!(duplicate.head_version_id, None);
        assert_eq!(
            second.find_artifact(&other_user_id, &artifact_id).await?,
            None
        );

        let future = version(&future_id, &artifact_id, 2);
        let second_version = version(&second_id, &artifact_id, 1);
        let first_version = version(&first_id, &artifact_id, 1);
        assert_eq!(
            first.save_document_version(&user_id, &future).await?,
            future
        );
        assert_eq!(
            second
                .save_document_version(&user_id, &second_version)
                .await?,
            second_version
        );
        assert_eq!(
            first
                .save_document_version(&user_id, &first_version)
                .await?,
            first_version
        );

        assert_eq!(
            first
                .save_suggestions(
                    &user_id,
                    &[first_suggestion.clone(), future_suggestion.clone()],
                )
                .await?,
            vec![first_suggestion.clone(), future_suggestion.clone()]
        );
        assert_eq!(
            second
                .save_suggestions(&user_id, std::slice::from_ref(&first_suggestion))
                .await?,
            vec![first_suggestion.clone()]
        );
        let mut conflicting_suggestion = first_suggestion.clone();
        conflicting_suggestion.suggested_text = "Different sentence".to_string();
        assert_eq!(
            second
                .save_suggestions(&user_id, &[conflicting_suggestion])
                .await,
            Err(PersistenceError::Conflict)
        );
        assert_eq!(
            first
                .get_suggestions_by_document_id(&user_id, &artifact_id)
                .await?
                .iter()
                .map(|suggestion| suggestion.version_id.as_str())
                .collect::<Vec<_>>(),
            vec![first_id.as_str(), future_id.as_str()]
        );
        assert_eq!(
            first
                .get_suggestions_by_document_id(&other_user_id, &artifact_id)
                .await,
            Err(PersistenceError::NotFound)
        );

        let history = second.get_document_versions(&user_id, &artifact_id).await?;
        assert_eq!(
            history
                .iter()
                .map(|version| version.version_id.as_str())
                .collect::<Vec<_>>(),
            vec![first_id.as_str(), second_id.as_str(), future_id.as_str()]
        );
        assert_eq!(
            first
                .get_latest_document_version(&user_id, &artifact_id)
                .await?
                .unwrap()
                .version_id,
            future_id
        );

        let mut mismatch = first_version.clone();
        mismatch.content = Some(serde_json::json!({"different": true}));
        assert_eq!(
            second.save_document_version(&user_id, &mismatch).await,
            Err(PersistenceError::Conflict)
        );

        let removed = first
            .delete_document_versions_after(
                &user_id,
                &artifact_id,
                Utc.timestamp_opt(1, 0).unwrap(),
            )
            .await?;
        assert_eq!(
            removed
                .iter()
                .map(|version| version.version_id.as_str())
                .collect::<Vec<_>>(),
            vec![future_id.as_str()]
        );
        assert!(first
            .delete_document_versions_after(
                &user_id,
                &artifact_id,
                Utc.timestamp_opt(1, 0).unwrap(),
            )
            .await?
            .is_empty());
        assert_eq!(
            second
                .get_document_versions(&user_id, &artifact_id)
                .await?
                .iter()
                .map(|version| version.version_id.as_str())
                .collect::<Vec<_>>(),
            vec![first_id.as_str(), second_id.as_str()]
        );
        assert_eq!(
            second
                .get_latest_document_version(&user_id, &artifact_id)
                .await?
                .unwrap()
                .version_id,
            second_id
        );
        assert_eq!(
            second
                .get_suggestions_by_document_id(&user_id, &artifact_id)
                .await?
                .iter()
                .map(|suggestion| suggestion.id.as_str())
                .collect::<Vec<_>>(),
            vec![first_suggestion.id.as_str()]
        );

        let parent = db.parent_path(ARTIFACTS_COLLECTION, &artifact_id).unwrap();
        let stored: StoredVersionMarker = db
            .fluent()
            .select()
            .by_id_in(VERSIONS_COLLECTION)
            .parent(parent.as_ref())
            .obj()
            .one(&future_id)
            .await
            .unwrap()
            .unwrap();
        assert!(stored.cleanup_at.is_some());
        let stored_suggestion: StoredSuggestionMarker = db
            .fluent()
            .select()
            .by_id_in(SUGGESTIONS_COLLECTION)
            .parent(parent.as_ref())
            .obj()
            .one(&future_suggestion.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored_suggestion.version_id, future_id);
        assert!(stored_suggestion.cleanup_at.is_some());
        Ok(())
    }
    .await;

    for version_id in [&first_id, &second_id, &future_id] {
        delete_version(&db, &artifact_id, version_id).await;
    }
    delete_suggestion(&db, &artifact_id, &first_suggestion.id).await;
    delete_suggestion(&db, &artifact_id, &future_suggestion.id).await;
    delete_artifact(&db, &artifact_id).await;
    result.unwrap();
}

#[tokio::test]
async fn version_operations_report_missing_artifacts() {
    let db = connect().await;
    let repository = FirestoreArtifactRepository::new(db);
    let result = repository
        .save_document_version(&id("user"), &version(&id("version"), &id("missing"), 1))
        .await;
    assert_eq!(result, Err(PersistenceError::NotFound));
}
