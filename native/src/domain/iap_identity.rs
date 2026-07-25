use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    error::ValidationError,
    validation::{validate_subject, MAX_PAYLOAD_BYTES},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IapSubject(String);

impl IapSubject {
    pub fn new(value: impl AsRef<str>) -> Result<Self, ValidationError> {
        Ok(Self(validate_subject(value.as_ref())?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for IapSubject {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IapIdentity {
    pub subject: IapSubject,
    pub email: String,
}

impl IapIdentity {
    pub fn new(
        subject: impl AsRef<str>,
        email: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        let email = email.into().trim().to_string();
        if email.is_empty() {
            return Err(ValidationError::Empty { field: "email" });
        }
        if email.len() > MAX_PAYLOAD_BYTES {
            return Err(ValidationError::TooLong {
                field: "email",
                max_bytes: MAX_PAYLOAD_BYTES,
            });
        }
        Ok(Self {
            subject: IapSubject::new(subject)?,
            email,
        })
    }

    pub fn user_key(&self) -> String {
        iap_user_key(self.subject.as_str()).expect("IapSubject is already validated")
    }
}

/// Returns a stable UUID v5 key for an IAP subject.
pub fn iap_user_key(subject: &str) -> Result<String, ValidationError> {
    let subject = IapSubject::new(subject)?;
    Ok(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("chatbot:iap:{}", subject.as_str()).as_bytes(),
    )
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_are_trimmed_but_case_is_preserved() {
        assert_eq!(
            IapSubject::new("  Subject/A ").unwrap().as_str(),
            "Subject/A"
        );
        assert_ne!(iap_user_key("User"), iap_user_key("user"));
    }

    #[test]
    fn iap_keys_are_stable() {
        assert_eq!(iap_user_key("subject-1"), iap_user_key(" subject-1 "));
        assert_eq!(iap_user_key("subject-1").unwrap().len(), 36);
    }

    #[test]
    fn blank_subjects_are_rejected() {
        assert!(matches!(
            IapSubject::new("  "),
            Err(ValidationError::Empty { field: "subject" })
        ));
    }
}
