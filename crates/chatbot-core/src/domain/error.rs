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

#[cfg(test)]
mod tests {
    use super::ValidationError;

    #[test]
    fn validation_failures_remain_domain_validation_errors() {
        assert_eq!(
            ValidationError::Empty { field: "id" }.to_string(),
            "id must not be empty"
        );
    }
}
