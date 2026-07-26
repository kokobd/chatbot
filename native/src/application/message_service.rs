use std::sync::Arc;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::application::repository::{MessageQuery, MessageRepository, PersistenceError};
use crate::domain::{Message, ValidationError};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MessageServiceError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

pub struct MessageService {
    repository: Arc<dyn MessageRepository>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::MessageRepository;
    use crate::domain::MessageRole;
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeMessageRepository {
        messages: Mutex<HashMap<String, Message>>,
    }

    #[async_trait]
    impl MessageRepository for FakeMessageRepository {
        async fn save_messages(
            &self,
            messages: &[Message],
        ) -> Result<Vec<Message>, PersistenceError> {
            let mut stored = self.messages.lock().unwrap();
            for message in messages {
                if let Some(existing) = stored.get(&message.id) {
                    if existing != message {
                        return Err(PersistenceError::Conflict);
                    }
                } else {
                    stored.insert(message.id.clone(), message.clone());
                }
            }
            Ok(messages.to_vec())
        }

        async fn update_message(&self, message: &Message) -> Result<Message, PersistenceError> {
            let mut stored = self.messages.lock().unwrap();
            let current = stored
                .get_mut(&message.id)
                .ok_or(PersistenceError::NotFound)?;
            current.parts = message.parts.clone();
            Ok(current.clone())
        }

        async fn get_message_by_id(
            &self,
            user_id: &str,
            chat_id: &str,
            message_id: &str,
        ) -> Result<Option<Message>, PersistenceError> {
            Ok(self
                .messages
                .lock()
                .unwrap()
                .get(message_id)
                .filter(|message| message.user_id == user_id && message.chat_id == chat_id)
                .cloned())
        }

        async fn get_messages_by_chat_id(
            &self,
            query: &MessageQuery,
        ) -> Result<Vec<Message>, PersistenceError> {
            let mut messages: Vec<_> = self
                .messages
                .lock()
                .unwrap()
                .values()
                .filter(|message| {
                    message.user_id == query.user_id && message.chat_id == query.chat_id
                })
                .cloned()
                .collect();
            messages.sort_by_key(|message| (message.created_at, message.id.clone()));
            Ok(messages)
        }

        async fn count_user_messages(
            &self,
            user_id: &str,
            cutoff: DateTime<Utc>,
        ) -> Result<u64, PersistenceError> {
            Ok(self
                .messages
                .lock()
                .unwrap()
                .values()
                .filter(|message| {
                    message.user_id == user_id
                        && message.role == MessageRole::User
                        && message.created_at >= cutoff
                })
                .count() as u64)
        }
    }

    fn message(id: &str, seconds: i64) -> Message {
        Message::new(
            id,
            "chat-1",
            "user-1",
            MessageRole::User,
            json!([{ "text": id }]),
            json!([]),
            Utc.timestamp_opt(seconds, 0).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn service_delegates_validated_message_operations_to_the_port() {
        let repository = Arc::new(FakeMessageRepository::default());
        let service = MessageService::new(repository.clone());
        let first = message("first", 1);
        let second = message("second", 1);

        assert_eq!(
            service
                .save_messages(&[first.clone(), second])
                .await
                .unwrap()
                .len(),
            2
        );
        let ordered = service
            .get_messages_by_chat_id("user-1", "chat-1")
            .await
            .unwrap();
        assert_eq!(ordered[0].id, "first");
        assert_eq!(
            service
                .get_message_by_id("user-1", "chat-1", "first")
                .await
                .unwrap(),
            Some(first.clone())
        );

        let mut updated = first.clone();
        updated.parts = json!([{ "text": "updated" }]);
        assert_eq!(
            service.update_message(&updated).await.unwrap().parts,
            updated.parts
        );
        assert_eq!(
            service
                .count_user_messages("user-1", Utc.timestamp_opt(1, 0).unwrap())
                .await
                .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn service_rejects_invalid_message_query_values() {
        let service = MessageService::new(Arc::new(FakeMessageRepository::default()));
        assert!(matches!(
            service.get_messages_by_chat_id(" ", "chat-1").await,
            Err(MessageServiceError::Persistence(
                PersistenceError::InvalidInput(_)
            ))
        ));
    }
}

impl MessageService {
    pub fn new(repository: Arc<dyn MessageRepository>) -> Self {
        Self { repository }
    }

    pub async fn save_messages(
        &self,
        messages: &[Message],
    ) -> Result<Vec<Message>, MessageServiceError> {
        Ok(self.repository.save_messages(messages).await?)
    }

    pub async fn update_message(&self, message: &Message) -> Result<Message, MessageServiceError> {
        Ok(self.repository.update_message(message).await?)
    }

    pub async fn get_message_by_id(
        &self,
        user_id: &str,
        chat_id: &str,
        message_id: &str,
    ) -> Result<Option<Message>, MessageServiceError> {
        Ok(self
            .repository
            .get_message_by_id(user_id, chat_id, message_id)
            .await?)
    }

    pub async fn get_messages_by_chat_id(
        &self,
        user_id: &str,
        chat_id: &str,
    ) -> Result<Vec<Message>, MessageServiceError> {
        let query = MessageQuery::new(user_id, chat_id)?;
        Ok(self.repository.get_messages_by_chat_id(&query).await?)
    }

    pub async fn count_user_messages(
        &self,
        user_id: &str,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, MessageServiceError> {
        Ok(self.repository.count_user_messages(user_id, cutoff).await?)
    }

    pub async fn delete_messages_after(
        &self,
        user_id: &str,
        chat_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<Message>, MessageServiceError> {
        Ok(self
            .repository
            .delete_messages_after(user_id, chat_id, timestamp)
            .await?)
    }
}
