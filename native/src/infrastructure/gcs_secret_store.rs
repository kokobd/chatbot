use async_trait::async_trait;
use google_cloud_storage::client::Storage;

use crate::application::secrets::{SecretObjectStore, SecretStoreError, MAX_SECRET_OBJECT_BYTES};

pub struct GcsSecretObjectStore {
    client: Storage,
}

impl GcsSecretObjectStore {
    pub async fn new() -> Result<Self, SecretStoreError> {
        let client = Storage::builder()
            .build()
            .await
            .map_err(|error| SecretStoreError::Client(error.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl SecretObjectStore for GcsSecretObjectStore {
    async fn read_object(&self, path: &str) -> Result<Vec<u8>, SecretStoreError> {
        let (bucket, object) = parse_gcs_path(path)?;
        let mut response = self
            .client
            .read_object(format!("projects/_/buckets/{bucket}"), object.clone())
            .send()
            .await
            .map_err(|error| SecretStoreError::Read(format!("{path}: {error}")))?;

        let mut bytes = Vec::new();
        while let Some(chunk) = response.next().await {
            let chunk =
                chunk.map_err(|error| SecretStoreError::Read(format!("{path}: {error}")))?;
            if bytes.len().saturating_add(chunk.len()) > MAX_SECRET_OBJECT_BYTES {
                return Err(SecretStoreError::Read(format!(
                    "{path}: object exceeds the 1 MiB limit"
                )));
            }
            bytes.extend_from_slice(&chunk);
        }

        Ok(bytes)
    }
}

fn parse_gcs_path(path: &str) -> Result<(String, String), SecretStoreError> {
    let path = path
        .strip_prefix("gs://")
        .ok_or(SecretStoreError::InvalidPath)?;
    let (bucket, object) = path.split_once('/').ok_or(SecretStoreError::InvalidPath)?;

    if bucket.is_empty() || object.is_empty() || object.starts_with('/') {
        return Err(SecretStoreError::InvalidPath);
    }

    Ok((bucket.to_string(), object.to_string()))
}

#[cfg(test)]
mod tests {
    use super::parse_gcs_path;
    use crate::application::secrets::SecretStoreError;

    #[test]
    fn parses_bucket_and_nested_object() {
        assert_eq!(
            parse_gcs_path("gs://chatbot-secrets/test/app.json").unwrap(),
            ("chatbot-secrets".to_string(), "test/app.json".to_string())
        );
    }

    #[test]
    fn rejects_non_gcs_paths() {
        assert!(matches!(
            parse_gcs_path("https://storage.googleapis.com/bucket/app.json"),
            Err(SecretStoreError::InvalidPath)
        ));
    }
}
