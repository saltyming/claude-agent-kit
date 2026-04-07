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
pub enum BufferContent {
    Raw(String),
    Edit {
        file_path: String,
        old_string: String,
        new_string: String,
        mode: EditMode,
        match_index: Option<u32>,
        line_range: Option<(u32, u32)>,
    },
}

// ── Target resolution ────────────────────────────────────

pub struct ResolvedTarget {
    pub old_text: String,
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
            for line in file_content.split('\n') {
                let end_pos = pos + line.len();
                offsets.push((pos, end_pos));
                pos = end_pos + 1;
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
        let old_text = file_content[byte_start..byte_end].to_string();

        Ok(ResolvedTarget {
            old_text,
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
            old_text: old_string.to_string(),
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

pub fn diff_texts(target: &ResolvedTarget, new_string: &str, mode: &EditMode, file_content: &str) -> (String, String) {
    match mode {
        EditMode::Replace => (target.old_text.clone(), new_string.to_string()),
        EditMode::After => (
            target.old_text.clone(),
            format!("{}{}", target.old_text, new_string),
        ),
        EditMode::Before => (
            target.old_text.clone(),
            format!("{}{}", new_string, target.old_text),
        ),
        EditMode::Append => (
            file_content.to_string(),
            if file_content.ends_with('\n') {
                format!("{}{}", file_content, new_string)
            } else {
                format!("{}\n{}", file_content, new_string)
            },
        ),
    }
}

// ── Buffer param structs ──────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteParams {
    /// Name of the buffer
    pub name: String,
    /// Content to store in the buffer
    pub content: String,
    /// If provided, show unified diff against this file in the response
    pub file_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditBufferParams {
    /// Name of the buffer
    pub name: String,
    /// Path to the file to edit
    pub file_path: String,
    /// The exact text to find. Required for replace/after/before (unless line_start is used). Ignored for append.
    pub old_string: Option<String>,
    /// The replacement or insertion text
    pub new_string: String,
    /// Position mode: "replace" (default), "after" (insert after old_string), "before" (insert before old_string), "append" (append to end of file)
    pub position: Option<String>,
    /// Target the Nth occurrence of old_string (1-based). Without this, old_string must appear exactly once.
    pub match_index: Option<u32>,
    /// Target a line range instead of old_string (1-based). When provided, old_string is ignored.
    pub line_start: Option<u32>,
    /// End of line range (1-based, inclusive). Defaults to line_start if omitted.
    pub line_end: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// Name of the buffer to read. Provide either name or file_path, not both.
    pub name: Option<String>,
    /// Path to a file to read from disk. Output includes line numbers for use with workslate_edit's line_start/line_end.
    pub file_path: Option<String>,
    /// Show line numbers in output (default: true for file reads, ignored for buffer reads)
    pub line_numbers: Option<bool>,
    /// Start reading from this line number (1-based, inclusive). Only used with file_path.
    pub start_line: Option<u32>,
    /// Stop reading at this line number (1-based, inclusive). Only used with file_path.
    pub end_line: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Path to the file to search
    pub file_path: String,
    /// Search pattern (substring match by default, or regex if regex=true)
    pub pattern: String,
    /// Treat pattern as a regular expression (default: false, plain substring match)
    pub regex: Option<bool>,
    /// Number of context lines to show around each match (default: 2)
    pub context: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiffParams {
    /// Name of the buffer
    pub name: String,
    /// Path to the file to diff against. Required for raw buffers, ignored for edit buffers.
    pub file_path: Option<String>,
    /// If provided, diff only this section of the file against the buffer. Only used with raw buffers.
    pub old_string: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyParams {
    /// Name of the buffer to apply
    pub name: String,
    /// Path to the target file. Required for raw buffers, ignored for edit buffers.
    pub file_path: Option<String>,
    /// If provided, replace only this section. Only used with raw buffers.
    pub old_string: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClearParams {
    /// Name of the buffer to clear. If omitted, all buffers are cleared.
    pub name: Option<String>,
}
