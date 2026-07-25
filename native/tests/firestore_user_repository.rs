use std::env;
use std::sync::Arc;

use chatbot_native::domain::{IapIdentity, User};
use chatbot_native::{
    FirestoreUserRepository, IapUser, PersistenceError, UserRepository, UserService,
};
use chrono::{DateTime, Utc};
use firestore::{FirestoreDb, FirestoreDbOptions};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const USERS_COLLECTION: &str = "users";

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StoredUserWithMarker {
    id: String,
    email: String,
    #[serde(rename = "iapSubject")]
    iap_subject: String,
    #[serde(rename = "createdAt")]
    created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    updated_at: DateTime<Utc>,
    marker: String,
}

fn required_environment(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} must be configured for this test"))
}

fn database_configuration() -> (String, String) {
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

async fn delete_user(db: &FirestoreDb, user_id: &str) {
    let _ = db
        .fluent()
        .delete()
        .from(USERS_COLLECTION)
        .document_id(user_id)
        .execute()
        .await;
}

fn new_identity(email: &str) -> IapIdentity {
    IapIdentity::new(format!("plan-03-{}", Uuid::new_v4().simple()), email).unwrap()
}

#[tokio::test]
async fn round_trips_create_lookup_and_email_update() {
    let db = connect().await;
    let repository = FirestoreUserRepository::new(db.clone());
    let identity = new_identity("old@example.com");
    let expected = IapUser::from_identity(&identity, Utc::now()).unwrap();
    let user_id = expected.user().id.clone();

    let result: Result<(), PersistenceError> = async {
        assert_eq!(repository.create_iap_user(&expected).await?, expected);
        assert_eq!(
            repository.find_iap_user(&identity.subject).await?,
            Some(expected.clone())
        );

        let email = chatbot_native::Email::new("new@example.com").unwrap();
        let updated = repository
            .update_iap_email(&identity.subject, &email)
            .await?;
        assert_eq!(updated.user().email, "new@example.com");
        assert_eq!(updated.user().created_at, expected.user().created_at);
        Ok(())
    }
    .await;

    delete_user(&db, &user_id).await;
    result.unwrap();
}

#[tokio::test]
async fn duplicate_create_returns_conflict() {
    let db = connect().await;
    let repository = FirestoreUserRepository::new(db.clone());
    let identity = new_identity("user@example.com");
    let user = IapUser::from_identity(&identity, Utc::now()).unwrap();
    let user_id = user.user().id.clone();

    let result: Result<(), PersistenceError> = async {
        repository.create_iap_user(&user).await?;
        assert_eq!(
            repository.create_iap_user(&user).await,
            Err(PersistenceError::Conflict)
        );
        Ok(())
    }
    .await;

    delete_user(&db, &user_id).await;
    result.unwrap();
}

#[tokio::test]
async fn email_update_preserves_unrelated_fields_and_malformed_records_are_rejected() {
    let db = connect().await;
    let repository = FirestoreUserRepository::new(db.clone());
    let identity = new_identity("old@example.com");
    let user = User::from_iap_identity(&identity, Utc::now()).unwrap();
    let user_id = user.id.clone();

    let result: Result<(), PersistenceError> = async {
        let stored = StoredUserWithMarker {
            id: user.id.clone(),
            email: user.email.clone(),
            iap_subject: identity.subject.as_str().to_string(),
            created_at: user.created_at,
            updated_at: user.updated_at,
            marker: "must-survive-email-update".to_string(),
        };
        db.fluent()
            .insert()
            .into(USERS_COLLECTION)
            .document_id(&user.id)
            .object(&stored)
            .execute::<StoredUserWithMarker>()
            .await
            .unwrap();

        let email = chatbot_native::Email::new("new@example.com").unwrap();
        let updated = repository
            .update_iap_email(&identity.subject, &email)
            .await?;
        assert_eq!(updated.user().email, "new@example.com");

        let after: StoredUserWithMarker = db
            .fluent()
            .select()
            .by_id_in(USERS_COLLECTION)
            .obj()
            .one(&user.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.marker, "must-survive-email-update");

        let malformed_identity = new_identity("bad@example.com");
        let malformed = StoredUserWithMarker {
            id: "not-the-document-id".to_string(),
            email: malformed_identity.email.clone(),
            iap_subject: malformed_identity.subject.as_str().to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            marker: "malformed".to_string(),
        };
        let malformed_id = malformed_identity.user_key();
        db.fluent()
            .insert()
            .into(USERS_COLLECTION)
            .document_id(&malformed_id)
            .object(&malformed)
            .execute::<StoredUserWithMarker>()
            .await
            .unwrap();
        assert!(matches!(
            repository.find_iap_user(&malformed_identity.subject).await,
            Err(PersistenceError::CorruptData(_))
        ));
        delete_user(&db, &malformed_id).await;
        Ok(())
    }
    .await;

    delete_user(&db, &user_id).await;
    result.unwrap();
}

#[tokio::test]
async fn independent_service_instances_share_one_record() {
    let db = connect().await;
    let first_service = Arc::new(UserService::new(Arc::new(FirestoreUserRepository::new(
        db.clone(),
    ))));
    let second_service = Arc::new(UserService::new(Arc::new(FirestoreUserRepository::new(
        db.clone(),
    ))));
    let identity = new_identity("user@example.com");
    let user_id = identity.user_key();

    // TODO(plan-03): force both live reads to miss before either create, and
    // use recoverable cleanup so a panic cannot leak the test document.
    let (first, second) = tokio::join!(
        first_service.get_or_create_iap_user(&identity),
        second_service.get_or_create_iap_user(&identity),
    );

    assert_eq!(first.unwrap(), second.unwrap());
    delete_user(&db, &user_id).await;
}
