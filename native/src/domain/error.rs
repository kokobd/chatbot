use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} is too long (maximum {max_bytes} bytes)")]
    TooLong {
        field: &'static str,
        max_bytes: usize,
    },
    #[error("{field} contains invalid characters")]
    InvalidCharacters { field: &'static str },
    #[error("{value} is not a valid {kind}")]
    InvalidEnum { kind: &'static str, value: String },
    #[error("{field} is too large ({actual_bytes} bytes; maximum {max_bytes})")]
    PayloadTooLarge {
        field: &'static str,
        actual_bytes: usize,
        max_bytes: usize,
    },
    #[error("{field} is not valid JSON: {message}")]
    InvalidJson {
        field: &'static str,
        message: String,
    },
}

/// Errors shared by persistence ports and adapters.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PersistenceError {
    #[error("record not found")]
    NotFound,
    #[error("record already exists")]
    Conflict,
    #[error("persistence service is unavailable: {0}")]
    Unavailable(String),
    #[error("invalid persistence input: {0}")]
    InvalidInput(String),
    #[error("persistence serialization failed: {0}")]
    Serialization(String),
}

impl From<ValidationError> for PersistenceError {
    fn from(error: ValidationError) -> Self {
        Self::InvalidInput(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{PersistenceError, ValidationError};

    #[test]
    fn validation_failures_are_classified_as_invalid_input() {
        let error = PersistenceError::from(ValidationError::Empty { field: "id" });
        assert!(matches!(error, PersistenceError::InvalidInput(_)));
    }

    #[test]
    fn operational_error_categories_remain_distinct() {
        assert_eq!(PersistenceError::NotFound, PersistenceError::NotFound);
        assert_ne!(PersistenceError::Conflict, PersistenceError::NotFound);
    }
}
