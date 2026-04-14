use schemars::JsonSchema;
use serde::Deserialize;

// ── Buffer types ──────────────────────────────────────────

#[derive(Clone)]
pub enum EditMode {
    Replace,
    After,
    Before,
    Append,
}

#[derive(Clone)]
pub struct BufferContent {
    pub content: String,
    pub file_path: Option<String>,
    pub depends_on: Vec<String>,
    /// SHA-256 of the disk file contents at the moment this buffer was
    /// loaded or written. Used by workslate_apply to detect stale buffers
    /// (disk file modified out-of-band since the buffer was staged).
    /// None for pure-buffer writes with no target file.
    pub source_hash: Option<String>,
}

// ── Target resolution ────────────────────────────────────

pub struct ResolvedTarget {
    pub byte_start: usize,
    pub byte_end: usize,
}

pub fn resolve_target(
    file_content: &str,
    old_string: &str,
    match_index: Option<u32>,
    line_range: Option<(u32, u32)>,
) -> Result<ResolvedTarget, String> {
    if let Some((start, end)) = line_range {
        let line_offsets: Vec<(usize, usize)> = {
            let mut offsets = Vec::new();
            let mut pos = 0;
            for line in file_content.lines() {
                let end_pos = pos + line.len();
                offsets.push((pos, end_pos));
                pos = end_pos + 1; // skip \n
            }
            if offsets.is_empty() {
                offsets.push((0, 0));
            }
            offsets
        };

        let s = (start as usize).saturating_sub(1);
        let e = (end as usize).min(line_offsets.len());
        if s >= line_offsets.len() || s >= e {
            return Err(format!(
                "line range {}-{} out of bounds (file has {} lines)",
                start,
                end,
                line_offsets.len()
            ));
        }

        let byte_start = line_offsets[s].0;
        let byte_end = if e < line_offsets.len() {
            line_offsets[e - 1].1 + 1
        } else {
            line_offsets[e - 1].1
        };
        let byte_end = byte_end.min(file_content.len());

        Ok(ResolvedTarget {
            byte_start,
            byte_end,
        })
    } else {
        let matches: Vec<usize> = file_content
            .match_indices(old_string)
            .map(|(i, _)| i)
            .collect();
        if matches.is_empty() {
            return Err("old_string not found in file".to_string());
        }

        let idx = if let Some(n) = match_index {
            if n == 0 || n as usize > matches.len() {
                return Err(format!(
                    "match_index {} out of range (found {} occurrences)",
                    n,
                    matches.len()
                ));
            }
            n as usize - 1
        } else {
            if matches.len() > 1 {
                return Err(format!(
                    "old_string appears {} times (must be unique, or use match_index)",
                    matches.len()
                ));
            }
            0
        };

        let byte_start = matches[idx];
        let byte_end = byte_start + old_string.len();
        Ok(ResolvedTarget {
            byte_start,
            byte_end,
        })
    }
}

pub fn apply_mode(file_content: &str, target: &ResolvedTarget, new_string: &str, mode: &EditMode) -> String {
    match mode {
        EditMode::Replace => format!(
            "{}{}{}",
            &file_content[..target.byte_start],
            new_string,
            &file_content[target.byte_end..]
        ),
        EditMode::After => format!(
            "{}{}{}",
            &file_content[..target.byte_end],
            new_string,
            &file_content[target.byte_end..]
        ),
        EditMode::Before => format!(
            "{}{}{}",
            &file_content[..target.byte_start],
            new_string,
            &file_content[target.byte_start..]
        ),
        EditMode::Append => {
            if file_content.ends_with('\n') {
                format!("{}{}", file_content, new_string)
            } else {
                format!("{}\n{}", file_content, new_string)
            }
        }
    }
}

// ── Buffer param structs ──────────────────────────────────
//
// Notes for tool callers: array / boolean / integer fields below use lenient
// deserializers from the `lenient` module. They accept the native JSON type
// (preferred) and also JSON-encoded strings (e.g. `"true"` for bool, `"3"` for
// u32, `"[\"a\"]"` for arrays) as a tolerance shim. When tolerance fails, the
// error message tells the caller to pass a raw JSON value.

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteParams {
    /// Name of the buffer
    pub name: String,
    /// Content to store in the buffer
    pub content: String,
    /// If provided, show unified diff against this file in the response
    pub file_path: Option<String>,
    /// Buffer names that must be applied before this buffer (dependency ordering).
    /// JSON array of strings, e.g. `["buf-types", "buf-core"]`. Must be a JSON
    /// array — do NOT pass a stringified array like `"[\"buf-types\"]"`.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_vec_string")]
    pub depends_on: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditBufferParams {
    /// Name of the buffer
    pub name: String,
    /// Path to file. With file_path: loads from disk and edits. Without: edits existing buffer content.
    pub file_path: Option<String>,
    /// The exact text to find. Required for replace/after/before (unless line_start is used). Ignored for append.
    pub old_string: Option<String>,
    /// The replacement or insertion text
    pub new_string: String,
    /// Position mode: "replace" (default), "after" (insert after old_string), "before" (insert before old_string), "append" (append to end of file)
    pub position: Option<String>,
    /// Target the Nth occurrence of old_string (1-based, JSON integer like `2`).
    /// Without this, old_string must appear exactly once. Pass a raw number, not a string.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_u32")]
    pub match_index: Option<u32>,
    /// Target a line range instead of old_string (1-based, JSON integer).
    /// When provided, old_string is ignored. Pass a raw number, not a string.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_u32")]
    pub line_start: Option<u32>,
    /// End of line range (1-based, inclusive, JSON integer). Defaults to line_start if omitted.
    /// Pass a raw number, not a string.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_u32")]
    pub line_end: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// Name of the buffer to read. Provide either name or file_path, not both.
    pub name: Option<String>,
    /// Path to a file to read from disk. Output includes line numbers for use with workslate_edit's line_start/line_end.
    pub file_path: Option<String>,
    /// Show line numbers in output (JSON boolean; default: true for file reads, ignored for buffer reads).
    /// Pass raw `true` / `false`, not strings.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_bool")]
    pub line_numbers: Option<bool>,
    /// Start reading from this line number (1-based, inclusive, JSON integer). Only used with file_path.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_u32")]
    pub start_line: Option<u32>,
    /// Stop reading at this line number (1-based, inclusive, JSON integer). Only used with file_path.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_u32")]
    pub end_line: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Path to the file to search
    pub file_path: String,
    /// Search pattern (substring match by default, or regex if regex=true)
    pub pattern: String,
    /// Treat pattern as a regular expression (JSON boolean; default: false).
    /// Pass raw `true` / `false`, not strings.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_bool")]
    pub regex: Option<bool>,
    /// Number of context lines to show around each match (JSON integer; default: 2).
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_u32")]
    pub context: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiffParams {
    /// Name of the buffer
    pub name: String,
    /// Path to the file to diff against. Falls back to stored file_path in the buffer.
    pub file_path: Option<String>,
    /// If provided, diff only this section of the file against the buffer.
    pub old_string: Option<String>,
    /// If true (JSON boolean), return a one-line summary (e.g. "3 hunk(s), +47/-12 lines") instead of full diff.
    /// Pass raw `true` / `false`, not strings.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_bool")]
    pub summary: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyParams {
    /// Name of the buffer to apply
    pub name: String,
    /// Path to the target file. Falls back to stored file_path in the buffer.
    pub file_path: Option<String>,
    /// If provided, replace only this section of the file with buffer content.
    pub old_string: Option<String>,
    /// If true (JSON boolean), show final file content without actually writing to disk.
    /// Pass raw `true` / `false`, not strings.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_bool")]
    pub dry_run: Option<bool>,
    /// Override stale buffer detection (JSON boolean). When the disk file has changed since
    /// the buffer was loaded, apply refuses to write unless force=true.
    /// Pass raw `true` / `false`, not strings.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_bool")]
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClearParams {
    /// Name of the buffer to clear. Required unless `all` is true.
    pub name: Option<String>,
    /// If true (JSON boolean), clear ALL staged buffers. Destructive — requires explicit
    /// opt-in to prevent accidental wipes in shared/team staging areas.
    /// Pass raw `true` / `false`, not strings.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_bool")]
    pub all: Option<bool>,
}

