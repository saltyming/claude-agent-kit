#![allow(dead_code)] // copied verbatim from aside/workslate; some helpers unused here
//! Lenient serde deserializers for MCP tool parameters.
//!
//! Some calling agents JSON-encode array / boolean / integer fields (e.g.
//! `target_files: "[\"a.rs\"]"` or `allow_concurrent: "true"`). The default
//! serde error for this is `invalid type: string "...", expected ...`, which
//! does not hint at the correction. These deserializers accept both the native
//! JSON type and a string that parses back to it, and on failure they return an
//! error message that explicitly shows the expected JSON shape.
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
                    "expected JSON array of strings like [\"a.rs\", \"b.rs\"]: {}",
                    e
                ))
            }),
        Value::String(s) => serde_json::from_str::<Vec<String>>(&s)
            .map(Some)
            .map_err(|_| {
                D::Error::custom(format!(
                    "expected JSON array like [\"a.rs\", \"b.rs\"], got stringified value {:?} — \
                 send an actual JSON array, not a JSON-encoded string",
                    s
                ))
            }),
        other => Err(D::Error::custom(format!(
            "expected JSON array like [\"a.rs\", \"b.rs\"], got {} — \
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

/// Accepts a JSON non-negative integer that fits in `u32`, or a string that
/// parses to such an integer. Null becomes `None`.
pub fn lenient_opt_u32<'de, D>(d: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Value::deserialize(d)?;
    match v {
        Value::Null => Ok(None),
        Value::Number(ref n) => {
            if let Some(u) = n.as_u64() {
                if u <= u32::MAX as u64 {
                    Ok(Some(u as u32))
                } else {
                    Err(D::Error::custom(format!(
                        "expected JSON integer in 0..={}, got {}",
                        u32::MAX,
                        u
                    )))
                }
            } else {
                Err(D::Error::custom(format!(
                    "expected non-negative JSON integer, got {}",
                    n
                )))
            }
        }
        Value::String(s) => s.trim().parse::<u32>().map(Some).map_err(|_| {
            D::Error::custom(format!(
                "expected JSON integer like 3, got string {:?} — \
                 send a raw JSON number, not a string",
                s
            ))
        }),
        other => Err(D::Error::custom(format!(
            "expected JSON integer like 3, got {} — \
             send a raw JSON number, not {0}",
            describe_value(&other)
        ))),
    }
}
