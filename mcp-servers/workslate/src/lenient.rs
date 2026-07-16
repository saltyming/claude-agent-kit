//! Lenient serde deserializers for MCP tool parameters.
//!
//! Some calling agents JSON-encode boolean fields (e.g. `urgent: "true"`). The
//! default serde error for this is `invalid type: string "...", expected ...`,
//! which does not hint at the correction. These deserializers accept both the
//! native JSON type and a string that parses back to it, and on failure they
//! return an error message that explicitly shows the expected JSON shape.
//!
//! Use on `Option<T>` fields with `#[serde(default, deserialize_with = "...")]`
//! so that missing fields still become `None` (the deserializer is only
//! invoked when the key is present).

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

fn describe_value(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Accepts a JSON boolean, or the strings "true" / "false" (case-insensitive).
/// Null becomes `None`.
pub fn lenient_opt_bool<'de, D>(d: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Value::deserialize(d)?;
    match v {
        Value::Null => Ok(None),
        Value::Bool(b) => Ok(Some(b)),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            _ => Err(D::Error::custom(format!(
                "expected JSON boolean (true or false), got string {:?} — \
                 send a raw JSON boolean, not a string",
                s
            ))),
        },
        other => Err(D::Error::custom(format!(
            "expected JSON boolean (true or false), got {} — \
             send a raw JSON boolean, not {0}",
            describe_value(&other)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct BoolWrap {
        #[serde(default, deserialize_with = "lenient_opt_bool")]
        v: Option<bool>,
    }

    fn de_bool(json: &str) -> Result<BoolWrap, serde_json::Error> {
        serde_json::from_str(json)
    }

    // ── lenient_opt_bool ──────────────────────────────────

    #[test]
    fn bool_absent_is_none() {
        assert_eq!(de_bool("{}").unwrap(), BoolWrap { v: None });
    }

    #[test]
    fn bool_null_is_none() {
        assert_eq!(de_bool(r#"{"v": null}"#).unwrap(), BoolWrap { v: None });
    }

    #[test]
    fn bool_native() {
        assert_eq!(
            de_bool(r#"{"v": true}"#).unwrap(),
            BoolWrap { v: Some(true) }
        );
        assert_eq!(
            de_bool(r#"{"v": false}"#).unwrap(),
            BoolWrap { v: Some(false) }
        );
    }

    #[test]
    fn bool_stringified_is_tolerated() {
        assert_eq!(
            de_bool(r#"{"v": "true"}"#).unwrap(),
            BoolWrap { v: Some(true) }
        );
        assert_eq!(
            de_bool(r#"{"v": "FALSE"}"#).unwrap(),
            BoolWrap { v: Some(false) }
        );
    }

    #[test]
    fn bool_bad_string_error_has_hint() {
        let err = de_bool(r#"{"v": "nope"}"#).unwrap_err().to_string();
        assert!(
            err.contains("JSON boolean"),
            "error should hint at JSON boolean: {}",
            err
        );
        assert!(
            err.contains("not a string"),
            "error should say not string: {}",
            err
        );
    }

    #[test]
    fn bool_wrong_type_error_has_hint() {
        let err = de_bool(r#"{"v": 1}"#).unwrap_err().to_string();
        assert!(
            err.contains("JSON boolean"),
            "error should hint at JSON boolean: {}",
            err
        );
    }
}
