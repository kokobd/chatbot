use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{error::ValidationError, json::JsonValue, validate_identifier};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub document_id: String,
    pub created_at: DateTime<Utc>,
    pub content: Option<JsonValue>,
}

impl DocumentVersion {
    pub fn new(
        document_id: impl AsRef<str>,
        created_at: DateTime<Utc>,
        content: Option<JsonValue>,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            document_id: validate_identifier(document_id.as_ref())?,
            created_at,
            content,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::DocumentVersion;

    #[test]
    fn content_is_nullable() {
        let version = DocumentVersion::new("document-1", Utc::now(), None).unwrap();
        assert!(version.content.is_none());
    }
}
