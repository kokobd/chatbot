use std::env;
use std::fs;
use std::sync::Once;

use chatbot_native::domain::{Chat, LifecycleState, Visibility};
use chatbot_native::{
    ChatHistoryCursor, ChatHistoryQuery, ChatRepository, ChatTitle, FirestoreChatRepository,
    PersistenceError,
};
use chrono::{TimeZone, Utc};
use firestore::{FirestoreDb, FirestoreDbOptions};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const CHATS_COLLECTION: &str = "chats";
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
                let value = value.trim().trim_matches('"').trim_matches('\'');
                env::set_var(name.trim(), value);
            }
        }
    });
}

fn required_environment(name: &str) -> String {
    load_dotenv();
    env::var(name).unwrap_or_else(|_| panic!("{name} must be configured for this test"))
}

fn database_configuration() -> (String, String) {
    load_dotenv();
    assert!(
        env::var_os("FIRESTORE_EMULATOR_HOST").is_none(),
        "FIRESTORE_EMULATOR_HOST must not be set for real-GCP tests"
    );
    let project_id = required_environment("FIRESTORE_PROJECT_ID");
    let database_id = required_environment("FIRESTORE_DATABASE_ID");
    assert_ne!(database_id, "(default)");
    (project_id, database_id)
}

async fn connect() -> FirestoreDb {
    let (project_id, database_id) = database_configuration();
    let _ = rustls::crypto::ring::default_provider().install_default();
    FirestoreDb::with_options(FirestoreDbOptions::new(project_id).with_database_id(database_id))
        .await
        .unwrap()
}

async fn delete_chat(db: &FirestoreDb, chat_id: &str) {
    let _ = db
        .fluent()
        .delete()
        .from(CHATS_COLLECTION)
        .document_id(chat_id)
        .execute()
        .await;
}

fn new_id(label: &str) -> String {
    format!("plan-04-{label}-{}", Uuid::new_v4().simple())
}

fn chat(id: &str, user_id: &str, seconds: i64, title: &str) -> Chat {
    Chat::new(
        id,
        user_id,
        title,
        Visibility::Private,
        Utc.timestamp_opt(seconds, 0).unwrap(),
    )
    .unwrap()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MalformedChatDocument {
    id: String,
    #[serde(rename = "userId")]
    user_id: String,
    title: String,
    visibility: String,
    lifecycle: String,
    #[serde(rename = "createdAt")]
    created_at: chrono::DateTime<Utc>,
    #[serde(rename = "deletedAt")]
    deleted_at: Option<chrono::DateTime<Utc>>,
    #[serde(rename = "lifecycleRevision")]
    lifecycle_revision: i64,
}

#[tokio::test]
async fn round_trips_chat_updates_and_duplicate_winner_comparison() {
    let db = connect().await;
    let first = FirestoreChatRepository::new(db.clone());
    let second = FirestoreChatRepository::new(db.clone());
    let user_id = new_id("user");
    let chat_id = new_id("chat");
    let original = chat(&chat_id, &user_id, 1, "original");
    let conflict_id = new_id("conflict");
    let conflict = chat(&conflict_id, &user_id, 2, "conflict");
    let mut ids = vec![chat_id, conflict_id];

    let result: Result<(), PersistenceError> = async {
        assert_eq!(first.create_chat(&original).await?, original);
        assert_eq!(second.create_chat(&original).await?, original);
        assert_eq!(
            first.find_chat(&user_id, &original.id).await?,
            Some(original.clone())
        );
        assert_eq!(
            first.find_chat(&new_id("other-user"), &original.id).await?,
            None
        );

        let updated_title = first
            .update_chat_title(
                &user_id,
                &original.id,
                &ChatTitle::new("new title").unwrap(),
            )
            .await?;
        assert_eq!(updated_title.title, "new title");
        let updated_visibility = second
            .update_chat_visibility(&user_id, &original.id, Visibility::Public)
            .await?;
        assert_eq!(updated_visibility.visibility, Visibility::Public);
        let loaded = first.find_chat(&user_id, &original.id).await?.unwrap();
        assert_eq!(loaded.title, "new title");
        assert_eq!(loaded.visibility, Visibility::Public);

        assert_eq!(first.create_chat(&conflict).await?, conflict);
        let different_owner = Chat::new(
            &conflict.id,
            new_id("different-owner"),
            "conflict",
            Visibility::Private,
            conflict.created_at,
        )
        .unwrap();
        assert_eq!(
            second.create_chat(&different_owner).await,
            Err(PersistenceError::Conflict)
        );
        ids.push(different_owner.user_id);
        Ok(())
    }
    .await;

    for id in ids {
        delete_chat(&db, &id).await;
    }
    result.unwrap();
}

#[tokio::test]
async fn history_supports_both_cursor_directions_equal_timestamps_and_tombstones() {
    let db = connect().await;
    let repository = FirestoreChatRepository::new(db.clone());
    let user_id = new_id("history-user");
    let ids: Vec<_> = [
        ("a", 10, "a"),
        ("b", 10, "b"),
        ("c", 11, "c"),
        ("d", 12, "d"),
        ("other", 13, "other"),
    ]
    .into_iter()
    .map(|(label, seconds, title)| {
        let id = new_id(label);
        (id.clone(), chat(&id, &user_id, seconds, title))
    })
    .collect();

    let result: Result<(), PersistenceError> = async {
        for (_, chat) in &ids {
            let owner = if chat.title == "other" {
                new_id("other-user")
            } else {
                user_id.clone()
            };
            let chat = Chat::new(
                &chat.id,
                owner,
                &chat.title,
                chat.visibility,
                chat.created_at,
            )
            .unwrap();
            repository.create_chat(&chat).await?;
        }

        let first_query = ChatHistoryQuery::new(&user_id, 2, None, None).unwrap();
        let first = repository.list_chat_history(&first_query).await?;
        assert_eq!(first.chats.len(), 2);
        assert!(first.has_more);
        assert_eq!(first.chats[0].title, "d");
        assert_eq!(first.chats[1].title, "c");

        let older_cursor = ChatHistoryCursor::new(first.chats[1].position());
        let older_query = ChatHistoryQuery::new(&user_id, 2, None, Some(older_cursor)).unwrap();
        let older = repository.list_chat_history(&older_query).await?;
        assert_eq!(
            older
                .chats
                .iter()
                .map(|chat| chat.title.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
        assert!(!older.has_more);

        let newer_cursor = ChatHistoryCursor::new(older.chats[1].position());
        let newer_query = ChatHistoryQuery::new(&user_id, 2, Some(newer_cursor), None).unwrap();
        let newer = repository.list_chat_history(&newer_query).await?;
        assert_eq!(
            newer
                .chats
                .iter()
                .map(|chat| chat.title.as_str())
                .collect::<Vec<_>>(),
            vec!["d", "c"]
        );
        assert!(newer.has_more);

        repository.delete_chat(&user_id, &older.chats[0].id).await?;
        let after_delete = repository.list_chat_history(&first_query).await?;
        assert!(!after_delete.chats.iter().any(|chat| chat.title == "b"));
        assert_eq!(
            repository.find_chat(&user_id, &older.chats[0].id).await?,
            None
        );

        let missing = ChatHistoryCursor::new(
            Chat::new(
                new_id("missing"),
                &user_id,
                "missing",
                Visibility::Private,
                Utc.timestamp_opt(1, 0).unwrap(),
            )
            .unwrap()
            .position(),
        );
        assert_eq!(
            repository
                .list_chat_history(
                    &ChatHistoryQuery::new(&user_id, 2, None, Some(missing)).unwrap()
                )
                .await,
            Err(PersistenceError::NotFound)
        );
        Ok(())
    }
    .await;

    for (id, _) in ids {
        delete_chat(&db, &id).await;
    }
    result.unwrap();
}

#[tokio::test]
async fn independent_instances_preserve_concurrent_field_specific_updates() {
    let db = connect().await;
    let first = FirestoreChatRepository::new(db.clone());
    let second = FirestoreChatRepository::new(db.clone());
    let user_id = new_id("concurrent-user");
    let id = new_id("concurrent-chat");
    let original = chat(&id, &user_id, 1, "before");

    let result: Result<(), PersistenceError> = async {
        first.create_chat(&original).await?;
        let title = ChatTitle::new("after-title").unwrap();
        let (title, visibility) = tokio::join!(
            first.update_chat_title(&user_id, &id, &title),
            second.update_chat_visibility(&user_id, &id, Visibility::Public),
        );
        assert!(title.is_ok(), "title update failed: {title:?}");
        assert!(
            visibility.is_ok(),
            "visibility update failed: {visibility:?}"
        );
        let current = first.find_chat(&user_id, &id).await?.unwrap();
        assert_eq!(current.title, "after-title");
        assert_eq!(current.visibility, Visibility::Public);
        Ok(())
    }
    .await;

    delete_chat(&db, &id).await;
    result.unwrap();
}

#[tokio::test]
async fn malformed_records_are_rejected_at_the_adapter_boundary() {
    let db = connect().await;
    let repository = FirestoreChatRepository::new(db.clone());
    let user_id = new_id("malformed-user");
    let id = new_id("malformed-chat");
    let record = MalformedChatDocument {
        id: "wrong-id".to_string(),
        user_id: user_id.clone(),
        title: "malformed".to_string(),
        visibility: "private".to_string(),
        lifecycle: "active".to_string(),
        created_at: Utc.timestamp_opt(1, 0).unwrap(),
        deleted_at: None,
        lifecycle_revision: 0,
    };

    db.fluent()
        .insert()
        .into(CHATS_COLLECTION)
        .document_id(&id)
        .object(&record)
        .execute::<MalformedChatDocument>()
        .await
        .unwrap();
    let result = repository.find_chat(&user_id, &id).await;
    delete_chat(&db, &id).await;

    assert!(matches!(result, Err(PersistenceError::CorruptData(_))));
}

#[tokio::test]
async fn stale_writes_cannot_revive_a_tombstoned_chat() {
    let db = connect().await;
    let repository = FirestoreChatRepository::new(db.clone());
    let user_id = new_id("tombstone-user");
    let id = new_id("tombstone-chat");
    let original = chat(&id, &user_id, 1, "before");

    let result: Result<(), PersistenceError> = async {
        repository.create_chat(&original).await?;
        let deleted = repository.delete_chat(&user_id, &id).await?;
        assert_eq!(deleted.lifecycle, LifecycleState::Deleted);
        assert!(matches!(
            repository
                .update_chat_title(&user_id, &id, &ChatTitle::new("stale").unwrap())
                .await,
            Err(PersistenceError::FailedPrecondition(_))
        ));
        Ok(())
    }
    .await;

    delete_chat(&db, &id).await;
    result.unwrap();
}
