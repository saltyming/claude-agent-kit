//! MCP tool parameter schemas.
//!
//! `SubmitParams` carries the structured task spec (objective / target_files /
//! constraints / acceptance) alongside free-form prose (context / details), plus
//! the execution knobs (backend / model / sandbox / …). Array and boolean fields
//! use the lenient deserializers so a stringified value from a calling agent is
//! tolerated with a corrective error.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::lenient::{lenient_opt_bool, lenient_opt_u32, lenient_opt_vec_string};

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SubmitParams {
    /// Required. What the delegated step must accomplish — a clear, self-contained
    /// objective the backend agent will implement.
    pub objective: String,

    /// Required. Absolute path to the directory the backend runs in. Codex runs
    /// WRITE-CAPABLE here and may modify files. The server rejects any path that
    /// does not canonicalize within the project root (or a configured extra root).
    pub working_dir: String,

    /// Optional short label for the step (shown in lists).
    pub title: Option<String>,

    /// Optional list of files expected to change.
    #[serde(default, deserialize_with = "lenient_opt_vec_string")]
    pub target_files: Option<Vec<String>>,

    /// Optional hard do / don't rules the backend must respect.
    #[serde(default, deserialize_with = "lenient_opt_vec_string")]
    pub constraints: Option<Vec<String>>,

    /// Optional acceptance / verification criteria — how to know the step is done.
    #[serde(default, deserialize_with = "lenient_opt_vec_string")]
    pub acceptance: Option<Vec<String>>,

    /// Optional free-form background context.
    pub context: Option<String>,

    /// Optional free-form additional instructions.
    pub details: Option<String>,

    /// Backend to delegate to. Default "codex".
    pub backend: Option<String>,

    /// Optional model override forwarded to the backend CLI.
    pub model: Option<String>,

    /// Optional reasoning effort (low / medium / high / xhigh).
    pub reasoning_effort: Option<String>,

    /// Sandbox mode: "read-only" | "workspace-write" (default) | "danger-full-access"
    /// (the last is rejected unless the server enables it via DISPATCH_ALLOW_DANGER).
    pub sandbox: Option<String>,

    /// Optional grouping label so a plan's steps can be listed / cancelled as a unit.
    pub plan_id: Option<String>,

    /// Permit the backend to run when working_dir is not a git repository.
    #[serde(default, deserialize_with = "lenient_opt_bool")]
    pub skip_git_repo_check: Option<bool>,

    /// Override the one-active-dispatch-per-working_dir guard (allow a second
    /// concurrent run against the same directory). Default false.
    #[serde(default, deserialize_with = "lenient_opt_bool")]
    pub allow_concurrent: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct StatusParams {
    /// Task id, e.g. "d-7".
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ListParams {
    /// Optional plan_id filter.
    pub plan_id: Option<String>,
    /// Optional status filter (queued / running / succeeded / failed / cancelled / interrupted).
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct CancelParams {
    /// Cancel a single task by id.
    pub id: Option<String>,
    /// Or cancel every active task in a plan. Exactly one of id / plan_id is required.
    pub plan_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct BackendsParams {}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct LogsParams {
    /// Task id, e.g. "d-7".
    pub id: String,
    /// 1-based first rendered line to return. Omitted = tail (last lines).
    #[serde(default, deserialize_with = "lenient_opt_u32")]
    pub line_start: Option<u32>,
    /// 1-based last rendered line (inclusive). Omitted = through the end.
    #[serde(default, deserialize_with = "lenient_opt_u32")]
    pub line_end: Option<u32>,
    /// Curation categories to include: any of lifecycle / messages / tools / edits /
    /// reasoning. Default: lifecycle + messages + tools + edits (reasoning is off —
    /// codex reasoning is encrypted).
    #[serde(default, deserialize_with = "lenient_opt_vec_string")]
    pub kinds: Option<Vec<String>>,
    /// Return raw rollout JSONL (within the line range) instead of the curated view.
    #[serde(default, deserialize_with = "lenient_opt_bool")]
    pub raw: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SteerParams {
    /// Task id to steer — its codex session is resumed with the new instruction.
    pub id: String,
    /// The new instruction to send to the resumed session.
    pub instruction: String,
    /// Optional model override for the follow-up run.
    pub model: Option<String>,
    /// Optional reasoning effort for the follow-up run.
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct WaitParams {
    /// Task id to wait on, e.g. "d-7".
    pub id: String,
    /// Bounded wait budget in milliseconds (default 30000, capped at 120000). The call
    /// returns as soon as the task is terminal, or with `timed_out=true` at the cap —
    /// re-invoke to keep waiting. This is a bounded long-poll, never an unbounded block.
    #[serde(default, deserialize_with = "lenient_opt_u32")]
    pub timeout_ms: Option<u32>,
}
