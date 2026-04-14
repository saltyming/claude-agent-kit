//! Tool parameter structs for the aside MCP server.
//!
//! Each `aside_<backend>` tool shares the same shape so Claude picks a backend
//! by choosing a tool, not by threading a `backend` union through a single
//! params struct. Shared fields are factored through a small builder below.
//!
//! Boolean and integer fields accept native JSON and JSON-encoded strings via
//! the lenient deserializers in `crate::lenient` — same convention as workslate.

use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for `aside_codex` / `aside_gemini` / `aside_copilot`.
///
/// All three tools share this schema. Backend-specific behaviour (argv
/// construction, prompt transport) lives in `crate::backend`.
#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct AskParams {
    /// The question to put to the backend. Required.
    pub question: String,

    /// Optional additional framing on top of the transcript. Use this when you
    /// want to tell the backend something that is not in the conversation yet
    /// (e.g. "focus on correctness, not style"). Prepended to the prompt.
    pub context: Option<String>,

    /// Forward the current Claude Code session transcript to the backend.
    /// Defaults to `true` — mirrors the built-in `advisor()` tool which also
    /// auto-forwards the transcript. Opt out explicitly with `false` for
    /// decontextualised questions (e.g. "what is 2+2").
    ///
    /// Pass raw `true` / `false`, not strings.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_bool")]
    pub include_transcript: Option<bool>,

    /// When transcript forwarding is on, keep only this many recent messages
    /// (default 80). The rendered transcript is further capped at 100 KB by
    /// trimming from the front. Pass a raw JSON number, not a string.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_u32")]
    pub transcript_tail: Option<u32>,

    /// Forwarded to the backend CLI's `--model` / `-m` flag. If omitted and the
    /// user's preference rule file sets a default for this backend, Claude
    /// should pass it. If both are absent, the CLI uses its own default.
    pub model: Option<String>,

    /// Forwarded as a reasoning-effort flag where supported:
    /// * codex  → `-c model_reasoning_effort=<val>`
    /// * copilot → `--effort <val>`
    /// * gemini → currently no-op (the CLI does not expose this knob); the
    ///   server logs a one-line notice when a value is passed.
    ///
    /// Valid values: `low` / `medium` / `high` / `xhigh` (or blank).
    pub reasoning_effort: Option<String>,
}

/// Parameters for `aside_list`. No fields — returns what the server can detect.
#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ListParams {}
