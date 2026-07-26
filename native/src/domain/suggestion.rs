use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{error::ValidationError, validate_identifier};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub document_id: String,
    pub version_id: String,
    pub user_id: String,
    pub original_text: String,
    pub suggested_text: String,
    pub description: Option<String>,
    pub is_resolved: bool,
    pub created_at: DateTime<Utc>,
}

impl Suggestion {
    pub fn new(
        id: impl AsRef<str>,
        document_id: impl AsRef<str>,
        version_id: impl AsRef<str>,
        user_id: impl AsRef<str>,
        original_text: impl Into<String>,
        suggested_text: impl Into<String>,
        description: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ValidationError> {
        let original_text = original_text.into();
        let suggested_text = suggested_text.into();
        if original_text.is_empty() {
            return Err(ValidationError::Empty {
                field: "original_text",
            });
        }
        if suggested_text.is_empty() {
            return Err(ValidationError::Empty {
                field: "suggested_text",
            });
        }
        Ok(Self {
            id: validate_identifier(id.as_ref())?,
            document_id: validate_identifier(document_id.as_ref())?,
            version_id: validate_identifier(version_id.as_ref())?,
            user_id: validate_identifier(user_id.as_ref())?,
            original_text,
            suggested_text,
            description,
            is_resolved: false,
            created_at,
        })
    }

    pub fn with_resolved(mut self, is_resolved: bool) -> Self {
        self.is_resolved = is_resolved;
        self
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::Suggestion;

    #[test]
    fn version_identity_and_nullable_description_are_preserved() {
        let suggestion = Suggestion::new(
            "suggestion-1",
            "artifact-1",
            "version-1",
            "user-1",
            "original",
            "suggested",
            None,
            Utc::now(),
        )
        .unwrap();

        assert_eq!(suggestion.version_id, "version-1");
        assert!(suggestion.description.is_none());
        assert!(!suggestion.is_resolved);
    }
}
