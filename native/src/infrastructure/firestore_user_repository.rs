use async_trait::async_trait;
use chrono::{DateTime, Utc};
use firestore::errors::FirestoreError;
use firestore::{FirestoreDb, FirestoreTransformServerValue, FirestoreWritePrecondition};
use serde::{Deserialize, Serialize};

use crate::application::repository::error::PersistenceError;
use crate::application::repository::user_repository::{Email, IapUser, UserRepository};
use crate::domain::{iap_user_key, IapSubject, User};

pub const USERS_COLLECTION: &str = "users";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct UserDocument {
    pub(crate) id: String,
    pub(crate) email: String,
    #[serde(rename = "iapSubject")]
    pub(crate) iap_subject: String,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct UserEmailUpdate {
    email: String,
}

impl UserDocument {
    fn from_iap_user(user: &IapUser) -> Result<Self, PersistenceError> {
        let subject = user.subject().as_str();
        let user = user.user();
        let expected_id = iap_user_key(subject).map_err(PersistenceError::from)?;
        if expected_id != user.id {
            return Err(PersistenceError::InvalidInput(
                "user ID does not match the IAP subject".to_string(),
            ));
        }

        Ok(Self {
            id: user.id.clone(),
            email: user.email.clone(),
            iap_subject: subject.to_string(),
            created_at: user.created_at,
            updated_at: user.updated_at,
        })
    }

    fn into_iap_user(
        self,
        document_id: &str,
        expected_subject: &IapSubject,
    ) -> Result<IapUser, PersistenceError> {
        let expected_id = iap_user_key(expected_subject.as_str())
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        if document_id != expected_id {
            return Err(PersistenceError::CorruptData(
                "document ID does not match the IAP subject".to_string(),
            ));
        }
        if self.id != document_id {
            return Err(PersistenceError::CorruptData(
                "user ID does not match the document ID".to_string(),
            ));
        }
        if self.iap_subject != expected_subject.as_str() {
            return Err(PersistenceError::CorruptData(
                "stored IAP subject does not match the lookup subject".to_string(),
            ));
        }

        Email::new(&self.email)
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        let user = User::new(
            self.id,
            self.email,
            Some(&self.iap_subject),
            self.created_at,
            self.updated_at,
        )
        .map_err(|error| PersistenceError::CorruptData(error.to_string()))?;
        IapUser::new(expected_subject.clone(), user)
            .map_err(|error| PersistenceError::CorruptData(error.to_string()))
    }
}

pub struct FirestoreUserRepository {
    db: FirestoreDb,
}

impl FirestoreUserRepository {
    pub fn new(db: FirestoreDb) -> Self {
        Self { db }
    }

    fn document_id(subject: &IapSubject) -> Result<String, PersistenceError> {
        iap_user_key(subject.as_str()).map_err(PersistenceError::from)
    }

    async fn find_document(
        &self,
        subject: &IapSubject,
    ) -> Result<Option<IapUser>, PersistenceError> {
        let document_id = Self::document_id(subject)?;
        let document: Option<UserDocument> = self
            .db
            .fluent()
            .select()
            .by_id_in(USERS_COLLECTION)
            .obj()
            .one(&document_id)
            .await
            .map_err(|error| map_firestore_error(error, FirestoreOperation::Read))?;

        document
            .map(|document| document.into_iap_user(&document_id, subject))
            .transpose()
    }
}

#[async_trait]
impl UserRepository for FirestoreUserRepository {
    async fn find_iap_user(
        &self,
        subject: &IapSubject,
    ) -> Result<Option<IapUser>, PersistenceError> {
        self.find_document(subject).await
    }

    async fn create_iap_user(&self, user: &IapUser) -> Result<IapUser, PersistenceError> {
        let document = UserDocument::from_iap_user(user)?;
        let document_id = Self::document_id(user.subject())?;
        let document = FirestoreDb::serialize_to_doc("", &document)
            .map_err(|error| map_firestore_error(error, FirestoreOperation::CreateRequest))?;

        match self
            .db
            .fluent()
            .insert()
            .into(USERS_COLLECTION)
            .document_id(&document_id)
            .document(document)
            .execute()
            .await
        {
            Ok(_) => {
                // The input was validated before the write, and using the
                // raw-document API avoids a post-commit response
                // deserialization failure.
                Ok(user.clone())
            }
            Err(error) => {
                let mapped = map_firestore_error(error, FirestoreOperation::CreateCommit);
                if is_ambiguous_write(&mapped) {
                    self.reconcile_create(user.subject(), mapped).await
                } else {
                    Err(mapped)
                }
            }
        }
    }

    async fn update_iap_email(
        &self,
        subject: &IapSubject,
        email: &Email,
    ) -> Result<IapUser, PersistenceError> {
        let document_id = Self::document_id(subject)?;
        let update = UserEmailUpdate {
            email: email.as_str().to_string(),
        };

        let mut transaction = self
            .db
            .begin_transaction()
            .await
            .map_err(|error| map_firestore_error(error, FirestoreOperation::UpdateSetup))?;
        self.db
            .fluent()
            .update()
            .fields(["email"])
            .in_col(USERS_COLLECTION)
            .precondition(FirestoreWritePrecondition::Exists(true))
            .document_id(&document_id)
            .object(&update)
            .transforms(|builder| {
                builder.fields([builder
                    .field("updatedAt")
                    .server_value(FirestoreTransformServerValue::RequestTime)])
            })
            .add_to_transaction(&mut transaction)
            .map_err(|error| map_firestore_error(error, FirestoreOperation::UpdateSetup))?;

        match transaction.commit().await {
            Ok(_) => self
                .find_document(subject)
                .await?
                .ok_or(PersistenceError::NotFound),
            Err(error) => {
                let mapped = map_firestore_error(error, FirestoreOperation::UpdateCommit);
                if is_ambiguous_write(&mapped) {
                    self.reconcile_update(subject, email, mapped).await
                } else {
                    Err(mapped)
                }
            }
        }
    }
}

impl FirestoreUserRepository {
    async fn reconcile_create(
        &self,
        subject: &IapSubject,
        original: PersistenceError,
    ) -> Result<IapUser, PersistenceError> {
        reconcile_create_result(original, self.find_document(subject).await)
    }

    async fn reconcile_update(
        &self,
        subject: &IapSubject,
        email: &Email,
        original: PersistenceError,
    ) -> Result<IapUser, PersistenceError> {
        reconcile_update_result(original, email, self.find_document(subject).await)
    }
}

fn reconcile_create_result(
    original: PersistenceError,
    result: Result<Option<IapUser>, PersistenceError>,
) -> Result<IapUser, PersistenceError> {
    match result {
        Ok(Some(user)) => Ok(user),
        Ok(None) => Err(original),
        Err(error) => Err(original.with_reconciliation_failure(error)),
    }
}

fn reconcile_update_result(
    original: PersistenceError,
    email: &Email,
    result: Result<Option<IapUser>, PersistenceError>,
) -> Result<IapUser, PersistenceError> {
    match result {
        Ok(Some(user)) if user.user().email == email.as_str() => Ok(user),
        Ok(Some(_)) | Ok(None) => Err(original),
        Err(error) => Err(original.with_reconciliation_failure(error)),
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum FirestoreOperation {
    Read,
    CreateRequest,
    CreateCommit,
    CreateResponse,
    UpdateSetup,
    UpdateCommit,
    UpdateResponse,
}

fn map_firestore_error(error: FirestoreError, operation: FirestoreOperation) -> PersistenceError {
    match error {
        FirestoreError::DataConflictError(_) => PersistenceError::Conflict,
        FirestoreError::DataNotFoundError(_) => PersistenceError::NotFound,
        FirestoreError::SerializeError(error) => {
            map_serialization_error(error.to_string(), operation)
        }
        FirestoreError::DeserializeError(error) => match operation {
            FirestoreOperation::Read => PersistenceError::CorruptData(error.to_string()),
            FirestoreOperation::CreateCommit
            | FirestoreOperation::CreateResponse
            | FirestoreOperation::UpdateCommit
            | FirestoreOperation::UpdateResponse => unknown_outcome(error.to_string(), false),
            FirestoreOperation::CreateRequest | FirestoreOperation::UpdateSetup => {
                PersistenceError::Serialization(error.to_string())
            }
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

fn map_serialization_error(message: String, operation: FirestoreOperation) -> PersistenceError {
    match operation {
        FirestoreOperation::CreateResponse | FirestoreOperation::UpdateResponse => {
            unknown_outcome(message, false)
        }
        FirestoreOperation::Read
        | FirestoreOperation::CreateRequest
        | FirestoreOperation::CreateCommit
        | FirestoreOperation::UpdateSetup
        | FirestoreOperation::UpdateCommit => PersistenceError::Serialization(message),
    }
}

fn map_system_error(message: String, operation: FirestoreOperation) -> PersistenceError {
    match operation {
        FirestoreOperation::CreateCommit
        | FirestoreOperation::CreateResponse
        | FirestoreOperation::UpdateCommit
        | FirestoreOperation::UpdateResponse => unknown_outcome(message, false),
        FirestoreOperation::Read
        | FirestoreOperation::CreateRequest
        | FirestoreOperation::UpdateSetup => PersistenceError::Internal {
            message,
            retryable: false,
        },
    }
}

fn unknown_outcome(message: String, retryable: bool) -> PersistenceError {
    PersistenceError::OutcomeUnknown {
        message,
        retryable,
        reconciliation: None,
    }
}

fn is_ambiguous_write(error: &PersistenceError) -> bool {
    matches!(error, PersistenceError::OutcomeUnknown { .. })
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
        FirestoreOperation::Read if retryable => {
            PersistenceError::Unavailable { message, retryable }
        }
        FirestoreOperation::Read => PersistenceError::Internal { message, retryable },
        FirestoreOperation::CreateCommit
        | FirestoreOperation::CreateResponse
        | FirestoreOperation::UpdateCommit
        | FirestoreOperation::UpdateResponse => unknown_outcome(message, retryable),
        FirestoreOperation::CreateRequest | FirestoreOperation::UpdateSetup => {
            PersistenceError::Unavailable { message, retryable }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_ambiguous_write, map_firestore_error, reconcile_create_result, reconcile_update_result,
        FirestoreOperation, UserDocument,
    };
    use crate::application::repository::error::PersistenceError;
    use crate::application::repository::user_repository::IapUser;
    use crate::domain::validation::MAX_PAYLOAD_BYTES;
    use crate::domain::{iap_user_key, IapIdentity};
    use firestore::errors::{
        FirestoreDataConflictError, FirestoreDatabaseError, FirestoreError,
        FirestoreErrorPublicGenericDetails,
    };

    #[test]
    fn derives_the_same_document_key_as_the_domain() {
        let identity = IapIdentity::new("subject-1", "user@example.com").unwrap();
        assert_eq!(
            iap_user_key(identity.subject.as_str()).unwrap(),
            identity.user_key()
        );
    }

    #[test]
    fn maps_create_conflicts_to_the_stable_persistence_error() {
        let error = FirestoreError::DataConflictError(FirestoreDataConflictError::new(
            FirestoreErrorPublicGenericDetails::new("AlreadyExists".to_string()),
            "already exists".to_string(),
        ));

        assert_eq!(
            map_firestore_error(error, FirestoreOperation::CreateCommit),
            PersistenceError::Conflict
        );
    }

    #[test]
    fn maps_retryable_write_failures_to_unknown_outcomes() {
        let error = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
            FirestoreErrorPublicGenericDetails::new("Unavailable".to_string()),
            "temporary outage".to_string(),
            true,
        ));

        assert!(matches!(
            map_firestore_error(error, FirestoreOperation::CreateCommit),
            PersistenceError::OutcomeUnknown {
                retryable: true,
                ..
            }
        ));
    }

    #[test]
    fn maps_transaction_setup_failures_to_known_errors() {
        let error = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
            FirestoreErrorPublicGenericDetails::new("Unavailable".to_string()),
            "transaction setup outage".to_string(),
            true,
        ));

        assert!(matches!(
            map_firestore_error(error, FirestoreOperation::UpdateSetup),
            PersistenceError::Unavailable {
                retryable: true,
                ..
            }
        ));
    }

    #[test]
    fn preserves_provider_error_categories() {
        fn map(code: &str, operation: FirestoreOperation) -> PersistenceError {
            let error = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
                FirestoreErrorPublicGenericDetails::new(code.to_string()),
                "provider error".to_string(),
                false,
            ));
            map_firestore_error(error, operation)
        }

        assert!(matches!(
            map("PermissionDenied", FirestoreOperation::Read),
            PersistenceError::PermissionDenied(_)
        ));
        assert!(matches!(
            map("FailedPrecondition", FirestoreOperation::UpdateCommit),
            PersistenceError::FailedPrecondition(_)
        ));
        assert!(matches!(
            map("InvalidArgument", FirestoreOperation::CreateCommit),
            PersistenceError::InvalidInput(_)
        ));
        assert!(!is_ambiguous_write(&map(
            "PermissionDenied",
            FirestoreOperation::UpdateCommit
        )));
        assert!(!is_ambiguous_write(&map(
            "FailedPrecondition",
            FirestoreOperation::UpdateCommit
        )));
    }

    #[test]
    fn maps_post_commit_response_conversion_failures_to_unknown_outcomes() {
        let error = FirestoreError::DeserializeError(
            firestore::errors::FirestoreSerializationError::from_message("bad response"),
        );

        assert!(matches!(
            map_firestore_error(error, FirestoreOperation::UpdateCommit),
            PersistenceError::OutcomeUnknown {
                retryable: false,
                reconciliation: None,
                ..
            }
        ));
    }

    #[test]
    fn nonretryable_write_failures_still_have_unknown_outcomes() {
        let error = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
            FirestoreErrorPublicGenericDetails::new("DeadlineExceeded".to_string()),
            "deadline".to_string(),
            false,
        ));

        assert!(matches!(
            map_firestore_error(error, FirestoreOperation::CreateCommit),
            PersistenceError::OutcomeUnknown {
                retryable: false,
                ..
            }
        ));
    }

    #[test]
    fn nonretryable_read_failures_are_internal() {
        let error = FirestoreError::DatabaseError(FirestoreDatabaseError::new(
            FirestoreErrorPublicGenericDetails::new("Unknown".to_string()),
            "database failure".to_string(),
            false,
        ));

        assert!(matches!(
            map_firestore_error(error, FirestoreOperation::Read),
            PersistenceError::Internal {
                retryable: false,
                ..
            }
        ));
    }

    #[test]
    fn maps_read_deserialization_failures_to_corrupt_data() {
        let error = FirestoreError::DeserializeError(
            firestore::errors::FirestoreSerializationError::from_message("bad timestamp"),
        );

        assert!(matches!(
            map_firestore_error(error, FirestoreOperation::Read),
            PersistenceError::CorruptData(_)
        ));
    }

    #[test]
    fn reconciliation_preserves_unknown_when_the_record_is_missing() {
        let original = PersistenceError::OutcomeUnknown {
            message: "commit response lost".to_string(),
            retryable: true,
            reconciliation: None,
        };

        assert_eq!(
            reconcile_create_result(original.clone(), Ok(None)),
            Err(original)
        );
    }

    #[test]
    fn reconciliation_attaches_read_failures_to_the_original_unknown() {
        let original = PersistenceError::OutcomeUnknown {
            message: "commit response lost".to_string(),
            retryable: true,
            reconciliation: None,
        };
        let reconciliation_error = PersistenceError::PermissionDenied("read denied".to_string());

        let error =
            reconcile_create_result(original, Err(reconciliation_error.clone())).unwrap_err();
        assert!(matches!(
            &error,
            PersistenceError::OutcomeUnknown {
                retryable: true,
                reconciliation: Some(failure),
                ..
            } if **failure == reconciliation_error
        ));
        assert!(error.to_string().contains("reconciliation failed"));
    }

    #[test]
    fn update_reconciliation_requires_the_requested_email() {
        let identity = IapIdentity::new("subject-1", "old@example.com").unwrap();
        let user = IapUser::from_identity(&identity, chrono::Utc::now()).unwrap();
        let original = PersistenceError::OutcomeUnknown {
            message: "commit response lost".to_string(),
            retryable: true,
            reconciliation: None,
        };
        let email =
            crate::application::repository::user_repository::Email::new("new@example.com").unwrap();

        assert_eq!(
            reconcile_update_result(original.clone(), &email, Ok(Some(user.clone()))),
            Err(original.clone())
        );

        let mut updated = user.user().clone();
        updated.email = email.as_str().to_string();
        let updated = IapUser::new(user.subject().clone(), updated).unwrap();
        assert_eq!(
            reconcile_update_result(original, &email, Ok(Some(updated.clone()))),
            Ok(updated)
        );
    }

    #[test]
    fn rejects_a_document_with_an_invalid_identity_binding_as_corrupt_data() {
        let identity = IapIdentity::new("subject-1", "user@example.com").unwrap();
        let document = UserDocument {
            id: "wrong-id".to_string(),
            email: identity.email.clone(),
            iap_subject: identity.subject.as_str().to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert!(matches!(
            document.into_iap_user(&identity.user_key(), &identity.subject),
            Err(PersistenceError::CorruptData(_))
        ));
    }

    #[test]
    fn rejects_an_oversized_persisted_email_as_corrupt_data() {
        let identity = IapIdentity::new("subject-1", "user@example.com").unwrap();
        let document = UserDocument {
            id: identity.user_key(),
            email: "x".repeat(MAX_PAYLOAD_BYTES + 1),
            iap_subject: identity.subject.as_str().to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert!(matches!(
            document.into_iap_user(&identity.user_key(), &identity.subject),
            Err(PersistenceError::CorruptData(_))
        ));
    }

    #[test]
    fn application_iap_user_invariant_is_preserved_by_the_adapter_boundary() {
        let identity = IapIdentity::new("subject-1", "user@example.com").unwrap();
        let user = IapUser::from_identity(&identity, chrono::Utc::now()).unwrap();
        let document = UserDocument::from_iap_user(&user).unwrap();
        assert_eq!(document.iap_subject, identity.subject.as_str());
    }
}
