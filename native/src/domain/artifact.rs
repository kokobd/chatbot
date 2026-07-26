use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{error::ValidationError, json::JsonValue, validate_identifier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Text,
    Code,
    Image,
    Sheet,
}

impl ArtifactKind {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        match value {
            "text" => Ok(Self::Text),
            "code" => Ok(Self::Code),
            "image" => Ok(Self::Image),
            "sheet" => Ok(Self::Sheet),
            _ => Err(ValidationError::InvalidEnum {
                kind: "artifact kind",
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub kind: ArtifactKind,
    pub content: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub head_version_id: Option<String>,
}

impl Artifact {
    pub fn new(
        id: impl AsRef<str>,
        user_id: impl AsRef<str>,
        title: impl Into<String>,
        kind: ArtifactKind,
        content: Option<JsonValue>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, ValidationError> {
        let title = title.into().trim().to_string();
        if title.is_empty() {
            return Err(ValidationError::Empty { field: "title" });
        }
        Ok(Self {
            id: validate_identifier(id.as_ref())?,
            user_id: validate_identifier(user_id.as_ref())?,
            title,
            kind,
            content,
            created_at,
            head_version_id: None,
        })
    }

    pub fn with_head_version_id(
        mut self,
        version_id: Option<impl AsRef<str>>,
    ) -> Result<Self, ValidationError> {
        self.head_version_id = version_id
            .map(|version_id| validate_identifier(version_id.as_ref()))
            .transpose()?;
        Ok(self)
    }
}
