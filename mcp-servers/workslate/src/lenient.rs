//! Lenient serde deserializers for MCP tool parameters.
//!
//! Some calling agents JSON-encode array / boolean / integer fields (e.g.
//! `depends_on: "[\"ws:1\"]"` or `dry_run: "true"`). The default serde error
//! for this is `invalid type: string "...", expected ...`, which does not hint
//! at the correction. These deserializers accept both the native JSON type
//! and a string that parses back to it, and on failure they return an error
//! message that explicitly shows the expected JSON shape.
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

/// Accepts a JSON array of strings, or a string containing JSON that parses to
/// a `Vec<String>`. Null becomes `None`.
pub fn lenient_opt_vec_string<'de, D>(d: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Value::deserialize(d)?;
    match v {
        Value::Null => Ok(None),
        Value::Array(_) => serde_json::from_value::<Vec<String>>(v)
            .map(Some)
            .map_err(|e| {
                D::Error::custom(format!(
                    "expected JSON array of strings like [\"ws:1\", \"team:2\"]: {}",
                    e
                ))
            }),
        Value::String(s) => serde_json::from_str::<Vec<String>>(&s)
            .map(Some)
            .map_err(|_| {
                D::Error::custom(format!(
                    "expected JSON array like [\"ws:1\", \"team:2\"], got stringified value {:?} — \
                 send an actual JSON array, not a JSON-encoded string",
                    s
                ))
            }),
        other => Err(D::Error::custom(format!(
            "expected JSON array like [\"ws:1\", \"team:2\"], got {} — \
             send a JSON array, not {0}",
            describe_value(&other)
        ))),
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
    struct VecWrap {
        #[serde(default, deserialize_with = "lenient_opt_vec_string")]
        v: Option<Vec<String>>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct BoolWrap {
        #[serde(default, deserialize_with = "lenient_opt_bool")]
        v: Option<bool>,
    }

    fn de_vec(json: &str) -> Result<VecWrap, serde_json::Error> {
        serde_json::from_str(json)
    }
    fn de_bool(json: &str) -> Result<BoolWrap, serde_json::Error> {
        serde_json::from_str(json)
    }

    // ── lenient_opt_vec_string ────────────────────────────

    #[test]
    fn vec_absent_is_none() {
        assert_eq!(de_vec("{}").unwrap(), VecWrap { v: None });
    }

    #[test]
    fn vec_null_is_none() {
        assert_eq!(de_vec(r#"{"v": null}"#).unwrap(), VecWrap { v: None });
    }

    #[test]
    fn vec_native_array() {
        assert_eq!(
            de_vec(r#"{"v": ["ws:1", "team:2"]}"#).unwrap(),
            VecWrap {
                v: Some(vec!["ws:1".into(), "team:2".into()])
            }
        );
    }

    #[test]
    fn vec_stringified_array_is_tolerated() {
        // This is the main motivating case: agent sends `depends_on: "[\"ws:1\"]"`.
        assert_eq!(
            de_vec(r#"{"v": "[\"ws:1\"]"}"#).unwrap(),
            VecWrap {
                v: Some(vec!["ws:1".into()])
            }
        );
    }

    #[test]
    fn vec_bad_string_error_has_hint() {
        let err = de_vec(r#"{"v": "not-json"}"#).unwrap_err().to_string();
        assert!(
            err.contains("JSON array"),
            "error should hint at JSON array form: {}",
            err
        );
        assert!(
            err.contains("not a JSON-encoded string"),
            "error should tell sender not to stringify: {}",
            err
        );
    }

    #[test]
    fn vec_wrong_type_error_has_hint() {
        let err = de_vec(r#"{"v": 123}"#).unwrap_err().to_string();
        assert!(
            err.contains("JSON array"),
            "error should hint at JSON array form: {}",
            err
        );
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
