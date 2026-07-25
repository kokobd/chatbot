use std::sync::Arc;

use chrono::Utc;
use thiserror::Error;

use crate::application::repository::error::PersistenceError;
use crate::application::repository::user_repository::{Email, IapUser, UserRepository};
use crate::domain::{IapIdentity, User, ValidationError};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UserServiceError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

pub struct UserService {
    repository: Arc<dyn UserRepository>,
}

impl UserService {
    pub fn new(repository: Arc<dyn UserRepository>) -> Self {
        Self { repository }
    }

    pub async fn get_or_create_iap_user(
        &self,
        identity: &IapIdentity,
    ) -> Result<User, UserServiceError> {
        let email = Email::new(&identity.email)?;
        if let Some(user) = self.repository.find_iap_user(&identity.subject).await? {
            return self.synchronize_email(user, &email).await;
        }

        let user = IapUser::from_identity(identity, Utc::now())?;
        match self.repository.create_iap_user(&user).await {
            Ok(user) => self.synchronize_email(user, &email).await,
            Err(PersistenceError::Conflict) => {
                let user = self
                    .repository
                    .find_iap_user(&identity.subject)
                    .await?
                    .ok_or(PersistenceError::Conflict)?;

                self.synchronize_email(user, &email).await
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn synchronize_email(
        &self,
        user: IapUser,
        email: &Email,
    ) -> Result<User, UserServiceError> {
        if user.user().email == email.as_str() {
            return Ok(user.into_user());
        }

        Ok(self
            .repository
            .update_iap_email(user.subject(), email)
            .await?
            .into_user())
    }
}

#[cfg(test)]
mod tests {
    use super::{UserService, UserServiceError};
    use crate::application::repository::error::PersistenceError;
    use crate::application::repository::user_repository::{Email, IapUser, UserRepository};
    use crate::domain::IapIdentity;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::Barrier;

    #[derive(Clone, Default)]
    struct FakeUserRepository {
        users: Arc<Mutex<HashMap<String, IapUser>>>,
        create_conflicts: Arc<Mutex<usize>>,
        conflict_winner: Arc<Mutex<Option<IapUser>>>,
        ambiguous_create_user: Arc<Mutex<Option<IapUser>>>,
        create_error: Arc<Mutex<Option<PersistenceError>>>,
        first_reads_barrier: Option<Arc<Barrier>>,
        find_calls: Arc<AtomicUsize>,
        create_attempts: Arc<AtomicUsize>,
        unavailable: Arc<Mutex<Option<PersistenceError>>>,
    }

    impl FakeUserRepository {
        fn with_conflict_winner(winner: IapUser) -> Self {
            Self {
                create_conflicts: Arc::new(Mutex::new(1)),
                conflict_winner: Arc::new(Mutex::new(Some(winner))),
                ..Self::default()
            }
        }

        fn with_concurrent_first_reads() -> Self {
            Self {
                first_reads_barrier: Some(Arc::new(Barrier::new(2))),
                ..Self::default()
            }
        }

        fn with_error(error: PersistenceError) -> Self {
            Self {
                unavailable: Arc::new(Mutex::new(Some(error))),
                ..Self::default()
            }
        }

        fn with_ambiguous_create(user: IapUser) -> Self {
            Self {
                ambiguous_create_user: Arc::new(Mutex::new(Some(user))),
                create_error: Arc::new(Mutex::new(Some(PersistenceError::OutcomeUnknown {
                    message: "response lost".to_string(),
                    retryable: true,
                    reconciliation: None,
                }))),
                ..Self::default()
            }
        }

        fn with_unknown_create() -> Self {
            Self {
                create_error: Arc::new(Mutex::new(Some(PersistenceError::OutcomeUnknown {
                    message: "response lost".to_string(),
                    retryable: true,
                    reconciliation: None,
                }))),
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl UserRepository for FakeUserRepository {
        async fn find_iap_user(
            &self,
            subject: &crate::domain::IapSubject,
        ) -> Result<Option<IapUser>, PersistenceError> {
            if let Some(error) = self.unavailable.lock().unwrap().clone() {
                return Err(error);
            }

            let find_call = self.find_calls.fetch_add(1, Ordering::SeqCst);
            let result = self
                .users
                .lock()
                .unwrap()
                .values()
                .find(|user| user.subject().as_str() == subject.as_str())
                .cloned();
            if find_call < 2 {
                if let Some(barrier) = &self.first_reads_barrier {
                    barrier.wait().await;
                }
            }

            Ok(result)
        }

        async fn create_iap_user(&self, user: &IapUser) -> Result<IapUser, PersistenceError> {
            self.create_attempts.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = self.unavailable.lock().unwrap().clone() {
                return Err(error);
            }

            if let Some(error) = self.create_error.lock().unwrap().clone() {
                if let Some(committed) = self.ambiguous_create_user.lock().unwrap().take() {
                    self.users
                        .lock()
                        .unwrap()
                        .insert(committed.user().id.clone(), committed);
                    return Ok(self
                        .users
                        .lock()
                        .unwrap()
                        .get(&user.user().id)
                        .cloned()
                        .expect("ambiguous fake commit must be readable"));
                }
                return Err(error);
            }

            let mut conflicts = self.create_conflicts.lock().unwrap();
            if *conflicts > 0 {
                *conflicts -= 1;
                if let Some(winner) = self.conflict_winner.lock().unwrap().take() {
                    self.users
                        .lock()
                        .unwrap()
                        .insert(winner.user().id.clone(), winner);
                }
                return Err(PersistenceError::Conflict);
            }

            let mut users = self.users.lock().unwrap();
            if users.contains_key(&user.user().id) {
                return Err(PersistenceError::Conflict);
            }
            users.insert(user.user().id.clone(), user.clone());
            Ok(user.clone())
        }

        async fn update_iap_email(
            &self,
            subject: &crate::domain::IapSubject,
            email: &Email,
        ) -> Result<IapUser, PersistenceError> {
            if let Some(error) = self.unavailable.lock().unwrap().clone() {
                return Err(error);
            }

            let mut users = self.users.lock().unwrap();
            let user = users
                .values_mut()
                .find(|user| user.subject().as_str() == subject.as_str())
                .ok_or(PersistenceError::NotFound)?;
            if user.user().email == email.as_str() {
                return Ok(user.clone());
            }
            let mut updated = user.user().clone();
            updated.email = email.as_str().to_string();
            updated.updated_at = Utc::now();
            let updated =
                IapUser::new(user.subject().clone(), updated).map_err(PersistenceError::from)?;
            *user = updated.clone();
            Ok(updated)
        }
    }

    fn identity(subject: &str, email: &str) -> IapIdentity {
        IapIdentity::new(subject, email).unwrap()
    }

    #[tokio::test]
    async fn creates_then_reuses_one_user() {
        let repository = FakeUserRepository::default();
        let service = UserService::new(Arc::new(repository.clone()));
        let identity = identity("subject-1", "user@example.com");

        let first = service.get_or_create_iap_user(&identity).await.unwrap();
        let second = service.get_or_create_iap_user(&identity).await.unwrap();

        assert_eq!(first, second);
        assert_eq!(repository.users.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn changed_email_updates_the_existing_user() {
        let repository = FakeUserRepository::default();
        let service = UserService::new(Arc::new(repository));
        let first = service
            .get_or_create_iap_user(&identity("subject-1", "old@example.com"))
            .await
            .unwrap();

        let updated = service
            .get_or_create_iap_user(&identity("subject-1", "new@example.com"))
            .await
            .unwrap();

        assert_eq!(updated.id, first.id);
        assert_eq!(updated.created_at, first.created_at);
        assert_eq!(updated.email, "new@example.com");
        assert!(updated.updated_at >= first.updated_at);
    }

    #[tokio::test]
    async fn create_conflicts_are_resolved_by_rereading_the_winner() {
        let identity = identity("subject-1", "user@example.com");
        let expected = IapUser::from_identity(&identity, Utc::now()).unwrap();
        let expected_user = expected.user().clone();
        let repository = FakeUserRepository::with_conflict_winner(expected.clone());
        let service = UserService::new(Arc::new(repository.clone()));

        let actual = service.get_or_create_iap_user(&identity).await.unwrap();
        assert_eq!(actual, expected_user);
    }

    #[tokio::test]
    async fn repository_resolved_ambiguous_create_is_not_reconciled_again() {
        let identity = identity("subject-1", "user@example.com");
        let committed = IapUser::from_identity(&identity, Utc::now()).unwrap();
        let committed_user = committed.user().clone();
        let repository = FakeUserRepository::with_ambiguous_create(committed.clone());
        let service = UserService::new(Arc::new(repository.clone()));

        let actual = service.get_or_create_iap_user(&identity).await.unwrap();
        assert_eq!(actual, committed_user);
        assert_eq!(repository.users.lock().unwrap().len(), 1);
        assert_eq!(repository.create_attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unknown_repository_outcomes_are_preserved_by_the_service() {
        let repository = FakeUserRepository::with_unknown_create();
        let service = UserService::new(Arc::new(repository.clone()));

        let error = service
            .get_or_create_iap_user(&identity("subject-1", "user@example.com"))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            UserServiceError::Persistence(PersistenceError::OutcomeUnknown {
                retryable: true,
                reconciliation: None,
                ..
            })
        ));
        assert_eq!(repository.find_calls.load(Ordering::SeqCst), 1);
        assert_eq!(repository.create_attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_first_calls_return_one_user() {
        let repository = FakeUserRepository::with_concurrent_first_reads();
        let first_service = Arc::new(UserService::new(Arc::new(repository.clone())));
        let second_service = Arc::new(UserService::new(Arc::new(repository.clone())));
        let identity = identity("subject-1", "user@example.com");

        let (first, second) = tokio::join!(
            first_service.get_or_create_iap_user(&identity),
            second_service.get_or_create_iap_user(&identity),
        );

        let first = first.unwrap();
        let second = second.unwrap();
        assert_eq!(first, second);
        assert_eq!(repository.users.lock().unwrap().len(), 1);
        assert_eq!(repository.create_attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn unrelated_subjects_are_isolated() {
        let repository = FakeUserRepository::default();
        let service = UserService::new(Arc::new(repository.clone()));

        let first = service
            .get_or_create_iap_user(&identity("subject-1", "one@example.com"))
            .await
            .unwrap();
        let second = service
            .get_or_create_iap_user(&identity("subject-2", "two@example.com"))
            .await
            .unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(repository.users.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn persistence_errors_are_preserved() {
        let service = UserService::new(Arc::new(FakeUserRepository::with_error(
            PersistenceError::Unavailable {
                message: "database".to_string(),
                retryable: false,
            },
        )));

        let error = service
            .get_or_create_iap_user(&identity("subject-1", "user@example.com"))
            .await
            .unwrap_err();

        assert_eq!(
            error,
            UserServiceError::Persistence(PersistenceError::Unavailable {
                message: "database".to_string(),
                retryable: false,
            })
        );
    }
}
