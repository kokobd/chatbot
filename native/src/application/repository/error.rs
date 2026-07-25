use thiserror::Error;

use crate::domain::ValidationError;

/// Stable errors exposed by application repository ports.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PersistenceError {
    #[error("record not found")]
    NotFound,
    #[error("record already exists")]
    Conflict,
    #[error(
        "persistence write outcome is unknown (retryable={retryable}): {message}{reconciliation}",
        reconciliation = format_reconciliation_failure(reconciliation)
    )]
    OutcomeUnknown {
        message: String,
        retryable: bool,
        reconciliation: Option<Box<PersistenceError>>,
    },
    #[error("persistence service is unavailable (retryable={retryable}): {message}")]
    Unavailable { message: String, retryable: bool },
    #[error("persistence precondition failed: {0}")]
    FailedPrecondition(String),
    #[error("persistence permission denied: {0}")]
    PermissionDenied(String),
    #[error("invalid persistence input: {0}")]
    InvalidInput(String),
    #[error("persisted data is corrupt: {0}")]
    CorruptData(String),
    #[error("persistence serialization failed: {0}")]
    Serialization(String),
    #[error("persistence internal error (retryable={retryable}): {message}")]
    Internal { message: String, retryable: bool },
}

fn format_reconciliation_failure(failure: &Option<Box<PersistenceError>>) -> String {
    failure
        .as_deref()
        .map(|failure| format!("; reconciliation failed: {failure}"))
        .unwrap_or_default()
}

impl PersistenceError {
    pub fn with_reconciliation_failure(self, failure: PersistenceError) -> Self {
        match self {
            Self::OutcomeUnknown {
                message,
                retryable,
                reconciliation,
            } => Self::OutcomeUnknown {
                message: if reconciliation.is_some() {
                    format!("{message}; additional reconciliation failure: {failure}")
                } else {
                    message
                },
                retryable,
                reconciliation: reconciliation.or(Some(Box::new(failure))),
            },
            other => other,
        }
    }
}

impl From<ValidationError> for PersistenceError {
    fn from(error: ValidationError) -> Self {
        Self::InvalidInput(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::PersistenceError;

    #[test]
    fn operational_categories_remain_distinct() {
        assert_ne!(PersistenceError::Conflict, PersistenceError::NotFound);
        assert_ne!(
            PersistenceError::FailedPrecondition("stale write".to_string()),
            PersistenceError::Unavailable {
                message: "stale write".to_string(),
                retryable: false,
            }
        );
    }

    #[test]
    fn unknown_write_outcomes_preserve_retryability() {
        let error = PersistenceError::OutcomeUnknown {
            message: "response lost".to_string(),
            retryable: true,
            reconciliation: None,
        };
        assert!(error.to_string().contains("retryable=true"));
    }

    #[test]
    fn unknown_write_outcomes_can_attach_reconciliation_failures() {
        let error = PersistenceError::OutcomeUnknown {
            message: "response lost".to_string(),
            retryable: true,
            reconciliation: None,
        };
        let error = error.with_reconciliation_failure(PersistenceError::NotFound);

        assert!(matches!(
            error,
            PersistenceError::OutcomeUnknown {
                reconciliation: Some(failure),
                ..
            } if *failure == PersistenceError::NotFound
        ));
    }
}
