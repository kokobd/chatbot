use std::env;
use std::fs;
use std::sync::Once;

use chatbot_tools::domain::{
    Chat, JsonValue, Message, MessageRole, PaginationPosition, Visibility,
};
use chatbot_tools::{
    ChatRepository, FirestoreChatRepository, FirestoreMessageRepository, MessageQuery,
    MessageRepository, PersistenceError,
};
use chrono::{TimeZone, Utc};
use firestore::{FirestoreDb, FirestoreDbOptions};
use serde_json::json;
use uuid::Uuid;

const CHATS_COLLECTION: &str = "chats";
const MESSAGES_COLLECTION: &str = "messages";
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
    format!("plan-05-{label}-{}", Uuid::new_v4().simple())
}

fn chat(id: &str, user_id: &str, seconds: i64) -> Chat {
    Chat::new(
        id,
        user_id,
        "Plan 5 integration",
        Visibility::Private,
        Utc.timestamp_opt(seconds, 0).unwrap(),
    )
    .unwrap()
}

fn message(id: &str, chat_id: &str, user_id: &str, role: MessageRole, seconds: i64) -> Message {
    Message::new(
        id,
        chat_id,
        user_id,
        role,
        json!([{ "text": id }]),
        JsonValue::Array(vec![]),
        Utc.timestamp_opt(seconds, 0).unwrap(),
    )
    .unwrap()
}

async fn delete_message(db: &FirestoreDb, chat_id: &str, message_id: &str) {
    let parent = db.parent_path(CHATS_COLLECTION, chat_id).unwrap();
    let _ = db
        .fluent()
        .delete()
        .from(MESSAGES_COLLECTION)
        .parent(parent.as_ref())
        .document_id(message_id)
        .execute()
        .await;
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

#[tokio::test]
#[ignore = "requires Terraform test workspace and ADC"]
async fn messages_round_trip_order_update_duplicates_and_collection_group_usage() {
    let db = connect().await;
    let chats = FirestoreChatRepository::new(db.clone());
    let first = FirestoreMessageRepository::new(db.clone());
    let second = FirestoreMessageRepository::new(db.clone());
    let user_id = id("user");
    let chat_id = id("chat");
    let other_chat_id = id("other-chat");
    let first_chat = chat(&chat_id, &user_id, 1);
    let other_chat = chat(&other_chat_id, &user_id, 2);
    let first_id = id("first");
    let second_id = id("second");
    let old_id = id("old");
    let assistant_id = id("assistant");
    let mut ids = vec![
        (chat_id.clone(), first_id.clone()),
        (chat_id.clone(), second_id.clone()),
        (other_chat_id.clone(), old_id.clone()),
        (other_chat_id.clone(), assistant_id.clone()),
    ];

    let result: Result<(), PersistenceError> = async {
        chats.create_chat(&first_chat).await?;
        chats.create_chat(&other_chat).await?;

        let first_message = message(&first_id, &chat_id, &user_id, MessageRole::User, 10);
        let second_message = message(&second_id, &chat_id, &user_id, MessageRole::User, 10);
        assert_eq!(
            first
                .save_messages(&[second_message.clone(), first_message.clone()])
                .await?,
            vec![second_message.clone(), first_message.clone()]
        );
        assert_eq!(
            second
                .save_messages(std::slice::from_ref(&first_message))
                .await?,
            vec![first_message.clone()]
        );

        let mut mismatch = first_message.clone();
        mismatch.parts = json!([{ "text": "different" }]);
        assert_eq!(
            first.save_messages(&[mismatch]).await,
            Err(PersistenceError::Conflict)
        );

        let ordered = first
            .get_messages_by_chat_id(&MessageQuery::new(&user_id, &chat_id).unwrap())
            .await?;
        assert_eq!(
            ordered
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![first_id.as_str(), second_id.as_str()]
        );

        let mut updated = first_message.clone();
        updated.parts = json!([{ "text": "updated" }]);
        assert_eq!(first.update_message(&updated).await?, updated);
        assert_eq!(
            first
                .get_message_by_id(&user_id, &chat_id, &first_id)
                .await?,
            Some(updated)
        );

        first
            .save_messages(&[
                message(&old_id, &other_chat_id, &user_id, MessageRole::User, 5),
                message(
                    &assistant_id,
                    &other_chat_id,
                    &user_id,
                    MessageRole::Assistant,
                    20,
                ),
            ])
            .await?;
        assert_eq!(
            first
                .count_user_messages(&user_id, Utc.timestamp_opt(9, 0).unwrap())
                .await?,
            2
        );

        chats.delete_chat(&user_id, &other_chat_id).await?;
        assert!(matches!(
            first
                .get_messages_by_chat_id(&MessageQuery::new(&user_id, &other_chat_id).unwrap())
                .await,
            Err(PersistenceError::FailedPrecondition(_))
        ));
        Ok(())
    }
    .await;

    for (chat_id, message_id) in ids.drain(..) {
        delete_message(&db, &chat_id, &message_id).await;
    }
    delete_chat(&db, &chat_id).await;
    delete_chat(&db, &other_chat_id).await;
    result.unwrap();
}

#[tokio::test]
#[ignore = "requires Terraform test workspace and ADC"]
async fn delete_messages_from_position_is_inclusive_and_uses_the_full_position() {
    let db = connect().await;
    let chats = FirestoreChatRepository::new(db.clone());
    let messages = FirestoreMessageRepository::new(db.clone());
    let owner = id("delete-owner");
    let other_user = id("delete-other-user");
    let chat_id = id("delete-chat");
    let other_chat_id = id("delete-other-chat");
    let deleted_chat_id = id("delete-tombstone-chat");
    let suffix = Uuid::new_v4().simple().to_string();
    let anchor_id = format!("a-anchor-{suffix}");
    let same_timestamp_id = format!("b-same-timestamp-{suffix}");
    let later_id = format!("c-later-{suffix}");
    let before_id = format!("z-before-{suffix}");
    let other_message_id = id("delete-other-message");
    let deleted_message_id = id("delete-tombstone-message");
    let cleanup = vec![
        (chat_id.clone(), before_id.clone()),
        (chat_id.clone(), anchor_id.clone()),
        (chat_id.clone(), same_timestamp_id.clone()),
        (chat_id.clone(), later_id.clone()),
        (other_chat_id.clone(), other_message_id.clone()),
        (deleted_chat_id.clone(), deleted_message_id.clone()),
    ];

    let result: Result<(), PersistenceError> = async {
        chats.create_chat(&chat(&chat_id, &owner, 1)).await?;
        chats.create_chat(&chat(&other_chat_id, &owner, 2)).await?;
        chats
            .create_chat(&chat(&deleted_chat_id, &owner, 3))
            .await?;

        messages
            .save_messages(&[
                message(&before_id, &chat_id, &owner, MessageRole::User, 9),
                message(&anchor_id, &chat_id, &owner, MessageRole::User, 10),
                message(
                    &same_timestamp_id,
                    &chat_id,
                    &owner,
                    MessageRole::Assistant,
                    10,
                ),
                message(&later_id, &chat_id, &owner, MessageRole::User, 11),
                message(
                    &other_message_id,
                    &other_chat_id,
                    &owner,
                    MessageRole::User,
                    12,
                ),
                message(
                    &deleted_message_id,
                    &deleted_chat_id,
                    &owner,
                    MessageRole::User,
                    13,
                ),
            ])
            .await?;

        let deleted = messages
            .delete_messages_from(
                &owner,
                &chat_id,
                &PaginationPosition::new(Utc.timestamp_opt(10, 0).unwrap(), anchor_id.clone()),
            )
            .await?;
        assert_eq!(deleted, 3);
        assert_eq!(
            messages
                .get_messages_by_chat_id(&MessageQuery::new(&owner, &chat_id).unwrap())
                .await?
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![before_id.as_str()]
        );
        assert_eq!(
            messages
                .get_messages_by_chat_id(&MessageQuery::new(&owner, &other_chat_id).unwrap())
                .await?
                .len(),
            1
        );

        let stale = messages
            .delete_messages_from(
                &owner,
                &chat_id,
                &PaginationPosition::new(Utc.timestamp_opt(8, 0).unwrap(), before_id.clone()),
            )
            .await;
        assert!(matches!(
            stale,
            Err(PersistenceError::FailedPrecondition(_))
        ));

        let missing = messages
            .delete_messages_from(
                &owner,
                &chat_id,
                &PaginationPosition::new(Utc.timestamp_opt(9, 0).unwrap(), "missing-message"),
            )
            .await;
        assert_eq!(missing, Err(PersistenceError::NotFound));

        let wrong_owner = messages
            .delete_messages_from(
                &other_user,
                &chat_id,
                &PaginationPosition::new(Utc.timestamp_opt(9, 0).unwrap(), before_id.clone()),
            )
            .await;
        assert_eq!(wrong_owner, Err(PersistenceError::NotFound));

        chats.delete_chat(&owner, &deleted_chat_id).await?;
        let tombstone = messages
            .delete_messages_from(
                &owner,
                &deleted_chat_id,
                &PaginationPosition::new(
                    Utc.timestamp_opt(13, 0).unwrap(),
                    deleted_message_id.clone(),
                ),
            )
            .await;
        assert!(matches!(
            tombstone,
            Err(PersistenceError::FailedPrecondition(_))
        ));
        Ok(())
    }
    .await;

    for (chat_id, message_id) in cleanup {
        delete_message(&db, &chat_id, &message_id).await;
    }
    delete_chat(&db, &chat_id).await;
    delete_chat(&db, &other_chat_id).await;
    delete_chat(&db, &deleted_chat_id).await;
    result.unwrap();
}

#[tokio::test]
#[ignore = "requires Terraform test workspace and ADC"]
async fn delete_messages_from_position_rejects_more_than_500_without_partial_writes() {
    let db = connect().await;
    let chats = FirestoreChatRepository::new(db.clone());
    let messages = FirestoreMessageRepository::new(db.clone());
    let owner = id("delete-limit-owner");
    let chat_id = id("delete-limit-chat");
    let suffix = Uuid::new_v4().simple().to_string();
    let branch: Vec<_> = (0..501)
        .map(|index| {
            message(
                &format!("branch-{index:03}-{suffix}"),
                &chat_id,
                &owner,
                MessageRole::User,
                20,
            )
        })
        .collect();

    let result: Result<(), PersistenceError> = async {
        chats.create_chat(&chat(&chat_id, &owner, 4)).await?;
        for chunk in branch.chunks(500) {
            messages.save_messages(chunk).await?;
        }

        let error = messages
            .delete_messages_from(&owner, &chat_id, &branch[0].position())
            .await
            .unwrap_err();
        assert_eq!(
            error,
            PersistenceError::FailedPrecondition(
                "message branch exceeds transaction write limit".to_string()
            )
        );
        assert_eq!(
            messages
                .get_messages_by_chat_id(&MessageQuery::new(&owner, &chat_id).unwrap())
                .await?
                .len(),
            501
        );

        delete_message(&db, &chat_id, &branch[0].id).await;
        assert_eq!(
            messages
                .delete_messages_from(&owner, &chat_id, &branch[1].position())
                .await?,
            500
        );
        assert!(messages
            .get_messages_by_chat_id(&MessageQuery::new(&owner, &chat_id).unwrap())
            .await?
            .is_empty());
        Ok(())
    }
    .await;

    for message in &branch {
        delete_message(&db, &chat_id, &message.id).await;
    }
    delete_chat(&db, &chat_id).await;
    result.unwrap();
}
