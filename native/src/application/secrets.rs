use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;

pub const MAX_SECRET_OBJECT_BYTES: usize = 1024 * 1024;

pub type SecretMap = BTreeMap<String, String>;

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("secret path must use the gs://bucket/object form")]
    InvalidPath,
    #[error("secret object client setup failed: {0}")]
    Client(String),
    #[error("secret object read failed: {0}")]
    Read(String),
}

#[async_trait]
pub trait SecretObjectStore: Send + Sync {
    async fn read_object(&self, path: &str) -> Result<Vec<u8>, SecretStoreError>;
}

#[derive(Debug, Error)]
pub enum SecretLoadError {
    #[error(transparent)]
    Store(#[from] SecretStoreError),
    #[error("secret object is not valid UTF-8")]
    InvalidUtf8,
    #[error("secret object is not valid JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    #[error("secret object exceeds the 1 MiB limit")]
    TooLarge,
    #[error("secret object must be a JSON object")]
    InvalidDocument,
    #[error("secret key is not a valid environment variable name: {0}")]
    InvalidKey(String),
    #[error("secret value for {0} must be a string")]
    InvalidValue(String),
}

pub struct SecretLoader<S> {
    store: S,
}

impl<S> SecretLoader<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
}

impl<S> SecretLoader<S>
where
    S: SecretObjectStore,
{
    pub async fn load(&self, path: &str) -> Result<SecretMap, SecretLoadError> {
        let bytes = self.store.read_object(path).await?;
        if bytes.len() > MAX_SECRET_OBJECT_BYTES {
            return Err(SecretLoadError::TooLarge);
        }

        let text = std::str::from_utf8(&bytes).map_err(|_| SecretLoadError::InvalidUtf8)?;
        let value: Value = serde_json::from_str(text).map_err(SecretLoadError::InvalidJson)?;
        let Value::Object(values) = value else {
            return Err(SecretLoadError::InvalidDocument);
        };

        values
            .into_iter()
            .map(|(key, value)| {
                validate_key(&key)?;
                let Value::String(value) = value else {
                    return Err(SecretLoadError::InvalidValue(key));
                };
                Ok((key, value))
            })
            .collect()
    }
}

fn validate_key(key: &str) -> Result<(), SecretLoadError> {
    let mut characters = key.chars();
    let Some(first) = characters.next() else {
        return Err(SecretLoadError::InvalidKey(key.to_string()));
    };

    if !(first == '_' || first.is_ascii_alphabetic())
        || characters.any(|character| !(character == '_' || character.is_ascii_alphanumeric()))
    {
        return Err(SecretLoadError::InvalidKey(key.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SecretLoadError, SecretLoader, SecretObjectStore, SecretStoreError};
    use async_trait::async_trait;

    struct FakeStore {
        bytes: Vec<u8>,
    }

    #[async_trait]
    impl SecretObjectStore for FakeStore {
        async fn read_object(&self, _path: &str) -> Result<Vec<u8>, SecretStoreError> {
            Ok(self.bytes.clone())
        }
    }

    #[tokio::test]
    async fn loads_string_values() {
        let loader = SecretLoader::new(FakeStore {
            bytes: br#"{"OPENROUTER_API_KEY":"secret"}"#.to_vec(),
        });

        let values = loader.load("gs://bucket/app.json").await.unwrap();

        assert_eq!(
            values.get("OPENROUTER_API_KEY"),
            Some(&"secret".to_string())
        );
    }

    #[tokio::test]
    async fn rejects_non_object_documents() {
        let loader = SecretLoader::new(FakeStore {
            bytes: br#"["secret"]"#.to_vec(),
        });

        assert!(matches!(
            loader.load("gs://bucket/app.json").await,
            Err(SecretLoadError::InvalidDocument)
        ));
    }

    #[tokio::test]
    async fn rejects_non_string_values() {
        let loader = SecretLoader::new(FakeStore {
            bytes: br#"{"PORT":3000}"#.to_vec(),
        });

        assert!(matches!(
            loader.load("gs://bucket/app.json").await,
            Err(SecretLoadError::InvalidValue(key)) if key == "PORT"
        ));
    }

    #[tokio::test]
    async fn rejects_invalid_environment_names() {
        let loader = SecretLoader::new(FakeStore {
            bytes: br#"{"bad-key":"secret"}"#.to_vec(),
        });

        assert!(matches!(
            loader.load("gs://bucket/app.json").await,
            Err(SecretLoadError::InvalidKey(key)) if key == "bad-key"
        ));
    }
}
