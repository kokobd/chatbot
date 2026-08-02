use serde_json::Value;

/// JSON is deliberately kept as an opaque value at the domain boundary.
pub type JsonValue = Value;

pub fn null_json() -> JsonValue {
    JsonValue::Null
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{null_json, JsonValue};

    #[test]
    fn preserves_arbitrary_json_and_null() {
        let value: JsonValue = json!({
            "nested": [true, 3, { "text": "value" }]
        });
        assert_eq!(value["nested"][2]["text"], "value");
        assert!(null_json().is_null());
    }
}
