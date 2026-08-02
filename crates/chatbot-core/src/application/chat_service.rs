use std::sync::Arc;

use thiserror::Error;

use crate::application::repository::{
    ChatHistoryPage, ChatHistoryQuery, ChatRepository, ChatTitle, PersistenceError,
};
use crate::domain::{Chat, ValidationError, Visibility, Vote};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChatServiceError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

pub struct ChatService {
    repository: Arc<dyn ChatRepository>,
}

impl ChatService {
    pub fn new(repository: Arc<dyn ChatRepository>) -> Self {
        Self { repository }
    }

    pub async fn create_chat(&self, chat: &Chat) -> Result<Chat, ChatServiceError> {
        Ok(self.repository.create_chat(chat).await?)
    }

    pub async fn find_chat(
        &self,
        user_id: &str,
        chat_id: &str,
    ) -> Result<Option<Chat>, ChatServiceError> {
        Ok(self.repository.find_chat(user_id, chat_id).await?)
    }

    pub async fn update_title(
        &self,
        user_id: &str,
        chat_id: &str,
        title: &str,
    ) -> Result<Chat, ChatServiceError> {
        let title = ChatTitle::new(title)?;
        Ok(self
            .repository
            .update_chat_title(user_id, chat_id, &title)
            .await?)
    }

    pub async fn update_visibility(
        &self,
        user_id: &str,
        chat_id: &str,
        visibility: Visibility,
    ) -> Result<Chat, ChatServiceError> {
        Ok(self
            .repository
            .update_chat_visibility(user_id, chat_id, visibility)
            .await?)
    }

    pub async fn delete_chat(
        &self,
        user_id: &str,
        chat_id: &str,
    ) -> Result<Chat, ChatServiceError> {
        Ok(self.repository.delete_chat(user_id, chat_id).await?)
    }

    pub async fn history(
        &self,
        query: &ChatHistoryQuery,
    ) -> Result<ChatHistoryPage, ChatServiceError> {
        Ok(self.repository.list_chat_history(query).await?)
    }

    pub async fn delete_all_chats_by_user(&self, user_id: &str) -> Result<u64, ChatServiceError> {
        Ok(self.repository.delete_all_chats_by_user(user_id).await?)
    }

    pub async fn upsert_vote(&self, user_id: &str, vote: &Vote) -> Result<Vote, ChatServiceError> {
        Ok(self.repository.upsert_vote(user_id, vote).await?)
    }

    pub async fn list_votes(
        &self,
        user_id: &str,
        chat_id: &str,
    ) -> Result<Vec<Vote>, ChatServiceError> {
        Ok(self.repository.list_votes(user_id, chat_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::ChatHistoryCursor;
    use crate::domain::{PaginationPosition, Visibility};
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeChatRepository {
        chats: Arc<Mutex<HashMap<String, Chat>>>,
    }

    #[async_trait]
    impl ChatRepository for FakeChatRepository {
        async fn find_chat(
            &self,
            user_id: &str,
            chat_id: &str,
        ) -> Result<Option<Chat>, PersistenceError> {
            Ok(self
                .chats
                .lock()
                .unwrap()
                .get(chat_id)
                .filter(|chat| {
                    chat.user_id == user_id
                        && chat.lifecycle != crate::domain::LifecycleState::Deleted
                })
                .cloned())
        }

        async fn create_chat(&self, chat: &Chat) -> Result<Chat, PersistenceError> {
            let mut chats = self.chats.lock().unwrap();
            match chats.get(&chat.id) {
                Some(existing) if existing == chat => Ok(existing.clone()),
                Some(_) => Err(PersistenceError::Conflict),
                None => {
                    chats.insert(chat.id.clone(), chat.clone());
                    Ok(chat.clone())
                }
            }
        }

        async fn update_chat_title(
            &self,
            user_id: &str,
            chat_id: &str,
            title: &ChatTitle,
        ) -> Result<Chat, PersistenceError> {
            let mut chats = self.chats.lock().unwrap();
            let chat = chats.get_mut(chat_id).ok_or(PersistenceError::NotFound)?;
            if chat.user_id != user_id {
                return Err(PersistenceError::NotFound);
            }
            if chat.lifecycle == crate::domain::LifecycleState::Deleted {
                return Err(PersistenceError::FailedPrecondition(
                    "chat is deleted".to_string(),
                ));
            }
            chat.title = title.as_str().to_string();
            chat.lifecycle_revision += 1;
            Ok(chat.clone())
        }

        async fn update_chat_visibility(
            &self,
            user_id: &str,
            chat_id: &str,
            visibility: Visibility,
        ) -> Result<Chat, PersistenceError> {
            let mut chats = self.chats.lock().unwrap();
            let chat = chats.get_mut(chat_id).ok_or(PersistenceError::NotFound)?;
            if chat.user_id != user_id {
                return Err(PersistenceError::NotFound);
            }
            if chat.lifecycle == crate::domain::LifecycleState::Deleted {
                return Err(PersistenceError::FailedPrecondition(
                    "chat is deleted".to_string(),
                ));
            }
            chat.visibility = visibility;
            chat.lifecycle_revision += 1;
            Ok(chat.clone())
        }

        async fn delete_chat(
            &self,
            user_id: &str,
            chat_id: &str,
        ) -> Result<Chat, PersistenceError> {
            let mut chats = self.chats.lock().unwrap();
            let chat = chats.get_mut(chat_id).ok_or(PersistenceError::NotFound)?;
            if chat.user_id != user_id {
                return Err(PersistenceError::NotFound);
            }
            if chat.lifecycle != crate::domain::LifecycleState::Deleted {
                chat.lifecycle = crate::domain::LifecycleState::Deleted;
                chat.deleted_at = Some(Utc::now());
                chat.lifecycle_revision += 1;
            }
            Ok(chat.clone())
        }

        async fn list_chat_history(
            &self,
            query: &ChatHistoryQuery,
        ) -> Result<ChatHistoryPage, PersistenceError> {
            let mut chats: Vec<_> = self
                .chats
                .lock()
                .unwrap()
                .values()
                .filter(|chat| {
                    chat.user_id == query.user_id
                        && chat.lifecycle != crate::domain::LifecycleState::Deleted
                })
                .cloned()
                .collect();
            chats.sort_by_key(|chat| std::cmp::Reverse(chat.position()));

            if let Some(cursor) = &query.starting_after {
                chats.retain(|chat| chat.position() > *cursor.position());
            }
            if let Some(cursor) = &query.ending_before {
                chats.retain(|chat| chat.position() < *cursor.position());
            }
            let has_more = chats.len() > query.limit as usize;
            chats.truncate(query.limit as usize);
            Ok(ChatHistoryPage { chats, has_more })
        }
    }

    fn chat(id: &str, user_id: &str, seconds: i64) -> Chat {
        Chat::new(
            id,
            user_id,
            format!("title-{id}"),
            Visibility::Private,
            Utc.timestamp_opt(seconds, 0).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn duplicate_create_is_idempotent_only_for_the_same_immutable_payload() {
        let repository = FakeChatRepository::default();
        let service = ChatService::new(Arc::new(repository));
        let first = chat("chat-1", "user-1", 1);

        assert_eq!(service.create_chat(&first).await.unwrap(), first);
        assert_eq!(service.create_chat(&first).await.unwrap(), first);

        let conflicting = Chat::new(
            "chat-1",
            "user-2",
            "different",
            Visibility::Public,
            first.created_at,
        )
        .unwrap();
        assert_eq!(
            service.create_chat(&conflicting).await,
            Err(ChatServiceError::Persistence(PersistenceError::Conflict))
        );
    }

    #[tokio::test]
    async fn history_filters_owners_and_tombstones_and_keeps_equal_timestamps_stable() {
        let repository = FakeChatRepository::default();
        let service = ChatService::new(Arc::new(repository.clone()));
        for chat in [
            chat("chat-a", "user-1", 1),
            chat("chat-b", "user-1", 1),
            chat("chat-c", "user-1", 2),
            chat("chat-other", "user-2", 3),
        ] {
            service.create_chat(&chat).await.unwrap();
        }
        service.delete_chat("user-1", "chat-b").await.unwrap();

        let query = ChatHistoryQuery::new("user-1", 1, None, None).unwrap();
        let first = service.history(&query).await.unwrap();
        assert_eq!(
            first
                .chats
                .iter()
                .map(|chat| chat.id.as_str())
                .collect::<Vec<_>>(),
            vec!["chat-c"]
        );
        assert!(first.has_more);

        let cursor = ChatHistoryCursor::new(PaginationPosition::new(
            first.chats[0].created_at,
            first.chats[0].id.clone(),
        ));
        let next_query = ChatHistoryQuery::new("user-1", 10, None, Some(cursor)).unwrap();
        let next = service.history(&next_query).await.unwrap();
        assert_eq!(
            next.chats
                .iter()
                .map(|chat| chat.id.as_str())
                .collect::<Vec<_>>(),
            vec!["chat-a"]
        );
    }

    #[tokio::test]
    async fn update_and_delete_are_fenced_by_the_tombstone() {
        let repository = FakeChatRepository::default();
        let service = ChatService::new(Arc::new(repository));
        let chat = chat("chat-1", "user-1", 1);
        service.create_chat(&chat).await.unwrap();
        service
            .update_title("user-1", "chat-1", "new title")
            .await
            .unwrap();
        service
            .update_visibility("user-1", "chat-1", Visibility::Public)
            .await
            .unwrap();
        let deleted = service.delete_chat("user-1", "chat-1").await.unwrap();
        assert_eq!(deleted.lifecycle, crate::domain::LifecycleState::Deleted);
        assert!(service
            .find_chat("user-1", "chat-1")
            .await
            .unwrap()
            .is_none());
        assert!(matches!(
            service.update_title("user-1", "chat-1", "stale").await,
            Err(ChatServiceError::Persistence(
                PersistenceError::FailedPrecondition(_)
            ))
        ));
    }

    #[test]
    fn cursor_round_trip_preserves_timestamp_and_id() {
        let position =
            PaginationPosition::new(Utc.timestamp_opt(1, 123_000_000).unwrap(), "chat-1");
        let cursor = ChatHistoryCursor::new(position.clone());
        let encoded = cursor.encode().unwrap();
        assert!(!encoded.contains("chat-1"));
        assert_eq!(
            ChatHistoryCursor::decode(encoded).unwrap().position(),
            &position
        );
    }
}
