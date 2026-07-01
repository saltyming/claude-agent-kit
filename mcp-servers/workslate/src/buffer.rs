use schemars::JsonSchema;
use serde::Deserialize;

// ── Buffer types ──────────────────────────────────────────

#[derive(Clone)]
pub enum EditMode {
    Replace,
    After,
    Before,
    AfterLine,
    BeforeLine,
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
        // (line_start, content_end) per line, where content_end excludes the line
        // terminator. Uses split_inclusive so the terminator length is counted exactly —
        // "\r\n" is two bytes, not one — otherwise byte offsets drift on CRLF files.
        let line_offsets: Vec<(usize, usize)> = {
            let mut offsets = Vec::new();
            let mut pos = 0;
            for piece in file_content.split_inclusive('\n') {
                let start = pos;
                pos += piece.len();
                let content = piece.strip_suffix('\n').unwrap_or(piece);
                let content = content.strip_suffix('\r').unwrap_or(content);
                offsets.push((start, start + content.len()));
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
        // End at the start of the line *after* the range (which includes this range's full
        // terminator, \n or \r\n); for the final line with no following line, stop at its
        // content end so a trailing terminator is preserved. On LF, line_offsets[e].0 equals
        // the old `content_end + 1`, so this is a no-op change for LF files.
        let byte_end = if e < line_offsets.len() {
            line_offsets[e].0
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

pub fn apply_mode(
    file_content: &str,
    target: &ResolvedTarget,
    new_string: &str,
    mode: &EditMode,
) -> String {
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
        EditMode::AfterLine => {
            // Insert new_string as its own line(s) after the whole line containing the match.
            // Inserting nothing is a no-op (an empty line-insert has no meaning; do not
            // add a stray newline the way the non-empty path would).
            if new_string.is_empty() {
                return file_content.to_string();
            }
            // The caller does not manage newlines: a trailing newline is added if absent.
            // If byte_end already sits at a line start (right after a newline — as with
            // line-range targeting, or an old_string that itself ends in '\n'), the anchor's
            // line has already ended there; insert at byte_end rather than skipping ahead to
            // the *next* line's newline.
            let at_line_boundary =
                target.byte_end > 0 && file_content.as_bytes()[target.byte_end - 1] == b'\n';
            let insert_at = if at_line_boundary {
                target.byte_end
            } else {
                match file_content[target.byte_end..].find('\n') {
                    Some(off) => target.byte_end + off + 1,
                    None => file_content.len(),
                }
            };
            let mut block = String::new();
            if insert_at == file_content.len() && !file_content.ends_with('\n') {
                block.push('\n');
            }
            block.push_str(new_string);
            if !block.ends_with('\n') {
                block.push('\n');
            }
            format!(
                "{}{}{}",
                &file_content[..insert_at],
                block,
                &file_content[insert_at..]
            )
        }
        EditMode::BeforeLine => {
            // Insert new_string as its own line(s) before the whole line containing the match.
            if new_string.is_empty() {
                return file_content.to_string();
            }
            // The caller does not manage newlines: a trailing newline is added if absent.
            // Symmetric to AfterLine: if byte_start points at a newline (the anchor begins
            // with a line terminator that belongs to the previous line), advance past it so
            // we anchor on the line the anchor's content is actually on.
            let byte_start = if target.byte_start < file_content.len()
                && file_content.as_bytes()[target.byte_start] == b'\n'
            {
                target.byte_start + 1
            } else {
                target.byte_start
            };
            let insert_at = file_content[..byte_start]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
            let mut block = String::from(new_string);
            if !block.ends_with('\n') {
                block.push('\n');
            }
            format!(
                "{}{}{}",
                &file_content[..insert_at],
                block,
                &file_content[insert_at..]
            )
        }
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
    /// The exact text to find. Required for replace/after/before/after_line/before_line (unless line_start is used). Ignored for append.
    pub old_string: Option<String>,
    /// The replacement or insertion text
    pub new_string: String,
    /// Position mode: "replace" (default), "after"/"before" (raw insert adjacent to old_string — caller manages newlines), "after_line"/"before_line" (insert new_string as its own line after/before the line containing old_string — the newline is handled for you), "append" (append to end of file)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn after_line(content: &str, anchor: &str, ins: &str) -> String {
        let t = resolve_target(content, anchor, None, None).unwrap();
        apply_mode(content, &t, ins, &EditMode::AfterLine)
    }
    fn before_line(content: &str, anchor: &str, ins: &str) -> String {
        let t = resolve_target(content, anchor, None, None).unwrap();
        apply_mode(content, &t, ins, &EditMode::BeforeLine)
    }

    #[test]
    fn after_line_inserts_whole_line_no_glue() {
        assert_eq!(after_line("A\nB\nC\n", "A", "X"), "A\nX\nB\nC\n");
    }

    #[test]
    fn after_line_does_not_double_a_supplied_trailing_newline() {
        assert_eq!(after_line("A\nB\n", "A", "X\n"), "A\nX\nB\n");
    }

    #[test]
    fn after_line_on_last_line_without_trailing_newline() {
        assert_eq!(after_line("A\nB", "B", "X"), "A\nB\nX\n");
    }

    #[test]
    fn after_line_on_last_line_with_trailing_newline() {
        assert_eq!(after_line("A\nB\n", "B", "X"), "A\nB\nX\n");
    }

    #[test]
    fn after_line_anchors_on_the_whole_line_not_the_match_offset() {
        // match is mid-line; insertion still lands after the entire line
        assert_eq!(
            after_line("foo bar\nbaz\n", "foo", "X"),
            "foo bar\nX\nbaz\n"
        );
    }

    #[test]
    fn after_line_with_line_range_targeting_inserts_after_that_line_not_the_next() {
        let t = resolve_target("A\nB\nC\n", "", None, Some((2, 2))).unwrap();
        assert_eq!(
            apply_mode("A\nB\nC\n", &t, "X", &EditMode::AfterLine),
            "A\nB\nX\nC\n"
        );
    }

    #[test]
    fn after_line_with_anchor_ending_in_newline() {
        let t = resolve_target("A\nB\nC\n", "B\n", None, None).unwrap();
        assert_eq!(
            apply_mode("A\nB\nC\n", &t, "X", &EditMode::AfterLine),
            "A\nB\nX\nC\n"
        );
    }

    #[test]
    fn before_line_inserts_whole_line_no_glue() {
        assert_eq!(before_line("A\nB\nC\n", "B", "X"), "A\nX\nB\nC\n");
    }

    #[test]
    fn before_line_on_first_line() {
        assert_eq!(before_line("A\nB\n", "A", "X"), "X\nA\nB\n");
    }

    #[test]
    fn before_line_multiline_block_each_on_own_line() {
        assert_eq!(before_line("A\nB\n", "B", "X\nY"), "A\nX\nY\nB\n");
    }

    #[test]
    fn before_line_with_anchor_starting_with_newline() {
        // anchor "\nB" begins with the newline that terminates the previous line; we still
        // insert before B's line, not A's (symmetric to AfterLine's boundary handling)
        let t = resolve_target("A\nB\nC\n", "\nB", None, None).unwrap();
        assert_eq!(
            apply_mode("A\nB\nC\n", &t, "X", &EditMode::BeforeLine),
            "A\nX\nB\nC\n"
        );
    }

    #[test]
    fn empty_new_string_is_a_noop_for_line_modes() {
        let after = resolve_target("A\nB\n", "A", None, None).unwrap();
        assert_eq!(
            apply_mode("A\nB\n", &after, "", &EditMode::AfterLine),
            "A\nB\n"
        );
        let before = resolve_target("A\nB\n", "B", None, None).unwrap();
        assert_eq!(
            apply_mode("A\nB\n", &before, "", &EditMode::BeforeLine),
            "A\nB\n"
        );
    }

    #[test]
    fn line_range_resolves_crlf_terminators_correctly() {
        // line 1 spans "A\r\n" (bytes 0..3), not "A\r" (0..2) — no CRLF offset drift
        let t1 = resolve_target("A\r\nB\r\n", "", None, Some((1, 1))).unwrap();
        assert_eq!((t1.byte_start, t1.byte_end), (0, 3));
        // last line: content "B" only (trailing terminator excluded, matching LF semantics)
        let t2 = resolve_target("A\r\nB\r\n", "", None, Some((2, 2))).unwrap();
        assert_eq!((t2.byte_start, t2.byte_end), (3, 4));
        // a multi-line range spans both full CRLF lines
        let t3 = resolve_target("A\r\nB\r\nC\r\n", "", None, Some((1, 2))).unwrap();
        assert_eq!((t3.byte_start, t3.byte_end), (0, 6));
    }

    #[test]
    fn line_range_lf_behavior_unchanged() {
        // full-line range includes the newline
        let a = resolve_target("A\nB\nC\n", "", None, Some((1, 2))).unwrap();
        assert_eq!((a.byte_start, a.byte_end), (0, 4));
        // last line excludes its trailing newline
        let b = resolve_target("A\nB\nC\n", "", None, Some((3, 3))).unwrap();
        assert_eq!((b.byte_start, b.byte_end), (4, 5));
    }

    #[test]
    fn raw_after_still_glues_documenting_the_footgun_the_line_modes_fix() {
        let t = resolve_target("A\nB\n", "A", None, None).unwrap();
        assert_eq!(apply_mode("A\nB\n", &t, "X", &EditMode::After), "AX\nB\n");
    }
}
