use serde::Serialize;

use super::error::ValidationError;

pub const MAX_IDENTIFIER_BYTES: usize = 256;
pub const MAX_SUBJECT_BYTES: usize = 512;
pub const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

pub fn validate_identifier(value: &str) -> Result<String, ValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ValidationError::Empty {
            field: "identifier",
        });
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(ValidationError::TooLong {
            field: "identifier",
            max_bytes: MAX_IDENTIFIER_BYTES,
        });
    }
    if value.chars().any(|character| character.is_control()) {
        return Err(ValidationError::InvalidCharacters {
            field: "identifier",
        });
    }
    Ok(value.to_string())
}

pub fn validate_subject(value: &str) -> Result<String, ValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ValidationError::Empty { field: "subject" });
    }
    if value.len() > MAX_SUBJECT_BYTES {
        return Err(ValidationError::TooLong {
            field: "subject",
            max_bytes: MAX_SUBJECT_BYTES,
        });
    }
    if value.chars().any(|character| character.is_control()) {
        return Err(ValidationError::InvalidCharacters { field: "subject" });
    }
    Ok(value.to_string())
}

pub fn validate_enum(
    value: &str,
    kind: &'static str,
    allowed: &[&str],
) -> Result<(), ValidationError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(ValidationError::InvalidEnum {
            kind,
            value: value.to_string(),
        })
    }
}

pub fn validate_payload_size(
    field: &'static str,
    payload: &[u8],
    max_bytes: usize,
) -> Result<(), ValidationError> {
    if payload.len() <= max_bytes {
        Ok(())
    } else {
        Err(ValidationError::PayloadTooLarge {
            field,
            actual_bytes: payload.len(),
            max_bytes,
        })
    }
}

pub fn validate_serialized_payload<T: Serialize>(
    field: &'static str,
    payload: &T,
    max_bytes: usize,
) -> Result<(), ValidationError> {
    let encoded = serde_json::to_vec(payload).map_err(|error| ValidationError::InvalidJson {
        field,
        message: error.to_string(),
    })?;
    validate_payload_size(field, &encoded, max_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_trimmed_and_control_characters_are_rejected() {
        assert_eq!(validate_identifier(" id ").unwrap(), "id");
        assert!(matches!(
            validate_identifier("id\nx"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn enum_validation_is_case_sensitive() {
        assert!(validate_enum("public", "visibility", &["public", "private"]).is_ok());
        assert!(validate_enum("Public", "visibility", &["public", "private"]).is_err());
    }

    #[test]
    fn payload_limits_include_the_boundary() {
        assert!(validate_payload_size("body", b"123", 3).is_ok());
        assert!(matches!(
            validate_payload_size("body", b"1234", 3),
            Err(ValidationError::PayloadTooLarge { .. })
        ));
    }
}
