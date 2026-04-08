mod buffer;
mod file;
mod task;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use regex::Regex;
use rmcp::{
    RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, Content, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    tool, tool_router,
};
use similar::{DiffOp, TextDiff};
use tokio::sync::RwLock;

use buffer::{
    ApplyParams, BufferContent, ClearParams, DiffParams, EditBufferParams, EditMode, ReadParams,
    ResolvedTarget, SearchParams, WriteParams, apply_mode, diff_texts, resolve_target,
};
use file::{MAX_FILE_SIZE, format_numbered_line, is_binary};
use task::{
    Namespace, TaskClearParams, TaskCreateParams, TaskDoneParams, TaskId, TaskInitParams,
    TaskListParams, TaskStatus, TaskUpdateParams, load_tasks,
    recompute_blocked_status, render_task_footer, serialize_depends_on, SCHEMA_SQL,
};

// ── Workslate server ──────────────────────────────────────

#[derive(Clone)]
struct Workslate {
    buffers: Arc<RwLock<HashMap<String, BufferContent>>>,
    applied_buffers: Arc<RwLock<HashSet<String>>>,
    db: Arc<StdMutex<rusqlite::Connection>>,
    tasks_dir: PathBuf,
    active_session: Arc<RwLock<Option<String>>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Workslate {
    fn new(db: rusqlite::Connection, tasks_dir: PathBuf) -> Self {
        Self {
            buffers: Arc::new(RwLock::new(HashMap::new())),
            applied_buffers: Arc::new(RwLock::new(HashSet::new())),
            db: Arc::new(StdMutex::new(db)),
            tasks_dir,
            active_session: Arc::new(RwLock::new(None)),
            tool_router: Self::tool_router(),
        }
    }

    // ── Buffer tools ──────────────────────────────────────

    #[tool(description = "Store content in a named raw buffer. If file_path is provided, returns the unified diff against that file for review.")]
    async fn workslate_write(
        &self,
        Parameters(params): Parameters<WriteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let line_count = params.content.lines().count();
        let header = format!("Buffer '{}' written ({} lines)", params.name, line_count);

        let diff_output = if let Some(ref file_path) = params.file_path {
            match tokio::fs::read_to_string(file_path).await {
                Ok(file_content) => {
                    let diff = TextDiff::from_lines(&file_content, &params.content);
                    let unified = diff
                        .unified_diff()
                        .context_radius(3)
                        .header(
                            &format!("a/{}", file_path),
                            &format!("b/{}", file_path),
                        )
                        .to_string();
                    if unified.is_empty() {
                        Some("No differences".to_string())
                    } else {
                        Some(unified)
                    }
                }
                Err(_) => {
                    let width = line_count.max(1).to_string().len();
                    let numbered: Vec<String> = params
                        .content
                        .lines()
                        .enumerate()
                        .map(|(i, line)| format_numbered_line(i + 1, width, line, false))
                        .collect();
                    Some(format!("(new file: {})\n{}", file_path, numbered.join("\n")))
                }
            }
        } else {
            None
        };

        let mut buffers = self.buffers.write().await;
        buffers.insert(params.name.clone(), BufferContent {
            content: params.content,
            file_path: params.file_path.clone(),
            depends_on: params.depends_on.unwrap_or_default(),
        });

        let output = match diff_output {
            Some(diff) => format!("{}\n{}", header, diff),
            None => header,
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Stage an edit. With file_path: loads file from disk and edits. Without file_path: edits existing buffer content. Modes: replace (default), after/before (insert around anchor), append. Targeting: old_string (unique), match_index (Nth occurrence), line_start/line_end (line range). Returns unified diff."
    )]
    async fn workslate_edit(
        &self,
        Parameters(params): Parameters<EditBufferParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mode = match params.position.as_deref() {
            None | Some("replace") => EditMode::Replace,
            Some("after") => EditMode::After,
            Some("before") => EditMode::Before,
            Some("append") => EditMode::Append,
            Some(other) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid position '{}'. Must be: replace, after, before, append",
                    other
                ))]));
            }
        };

        let line_range = params.line_start.map(|s| {
            let e = params.line_end.unwrap_or(s);
            (s, e)
        });

        let old_string = match mode {
            EditMode::Append => String::new(),
            _ if line_range.is_some() => String::new(),
            _ => match params.old_string {
                Some(ref s) if !s.is_empty() => s.clone(),
                _ => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "old_string is required (unless using line_start or append)".to_string(),
                    )]));
                }
            },
        };

        let (base_content, stored_file_path, stored_depends_on) = if let Some(ref file_path) = params.file_path {
            match tokio::fs::read_to_string(file_path).await {
                Ok(c) => (c, Some(file_path.clone()), vec![]),
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to read '{}': {}",
                        file_path, e
                    ))]));
                }
            }
        } else {
            let buffers = self.buffers.read().await;
            match buffers.get(&params.name) {
                Some(buf) => (buf.content.clone(), buf.file_path.clone(), buf.depends_on.clone()),
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "No buffer '{}' and no file_path provided",
                        params.name
                    ))]));
                }
            }
        };

        let diff_header_path = stored_file_path.as_deref().unwrap_or(&params.name);

        let target = if matches!(mode, EditMode::Append) {
            None
        } else {
            match resolve_target(&base_content, &old_string, params.match_index, line_range) {
                Ok(t) => Some(t),
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
        };

        let empty_target = ResolvedTarget {
            old_text: String::new(),
            byte_start: 0,
            byte_end: 0,
        };
        let target_ref = target.as_ref().unwrap_or(&empty_target);

        let (diff_old, diff_new) = diff_texts(target_ref, &params.new_string, &mode, &base_content);
        let result_content = apply_mode(&base_content, target_ref, &params.new_string, &mode);

        let diff = TextDiff::from_lines(&diff_old, &diff_new);
        let unified = diff
            .unified_diff()
            .context_radius(3)
            .header(
                &format!("a/{}", diff_header_path),
                &format!("b/{}", diff_header_path),
            )
            .to_string();

        let mut buffers = self.buffers.write().await;
        buffers.insert(
            params.name.clone(),
            BufferContent {
                content: result_content,
                file_path: stored_file_path,
                depends_on: stored_depends_on,
            },
        );

        let output = if unified.is_empty() {
            format!("Edit '{}' staged (no differences)", params.name)
        } else {
            format!("Edit '{}' staged\n{}", params.name, unified)
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Read a buffer by name, or read a file from disk with line numbers. Provide either name (buffer) or file_path (file), not both.")]
    async fn workslate_read(
        &self,
        Parameters(params): Parameters<ReadParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match (&params.name, &params.file_path) {
            (Some(name), None) => {
                let buffers = self.buffers.read().await;
                match buffers.get(name) {
                    Some(buf) => {
                        let header = match &buf.file_path {
                            Some(fp) => format!("[target: {}] {} lines", fp, buf.content.lines().count()),
                            None => format!("{} lines", buf.content.lines().count()),
                        };
                        Ok(CallToolResult::success(vec![Content::text(
                            format!("{}\n{}", header, buf.content),
                        )]))
                    }
                    None => Ok(CallToolResult::error(vec![Content::text(format!(
                        "Buffer '{}' not found",
                        name
                    ))])),
                }
            }
            (None, Some(file_path)) => {
                let metadata = match tokio::fs::metadata(file_path).await {
                    Ok(m) => m,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to read '{}': {}",
                            file_path, e
                        ))]));
                    }
                };
                if metadata.len() > MAX_FILE_SIZE {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "File too large ({} bytes, max {}). Use start_line/end_line to read a portion.",
                        metadata.len(),
                        MAX_FILE_SIZE
                    ))]));
                }

                let content_bytes = match tokio::fs::read(file_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to read '{}': {}",
                            file_path, e
                        ))]));
                    }
                };

                if is_binary(&content_bytes) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "File appears to be binary: {}",
                        file_path
                    ))]));
                }

                let text = String::from_utf8_lossy(&content_bytes);
                let all_lines: Vec<&str> = text.lines().collect();
                let total_lines = all_lines.len();

                if total_lines == 0 {
                    return Ok(CallToolResult::success(vec![Content::text(format!(
                        "{} (0 lines)",
                        file_path
                    ))]));
                }

                let start = params.start_line.map(|s| s as usize).unwrap_or(1);
                let end = params.end_line.map(|e| e as usize).unwrap_or(total_lines);

                if start == 0 || start > total_lines {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "start_line {} out of range (file has {} lines)",
                        start, total_lines
                    ))]));
                }
                if end < start {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "end_line {} is before start_line {}",
                        end, start
                    ))]));
                }
                let end = end.min(total_lines);

                let show_numbers = params.line_numbers.unwrap_or(true);
                let width = end.to_string().len();

                let mut output = Vec::with_capacity(end - start + 2);
                output.push(format!(
                    "{} ({} lines total, showing {}-{})",
                    file_path, total_lines, start, end
                ));

                for (i, line) in all_lines[(start - 1)..end].iter().enumerate() {
                    let line_num = start + i;
                    if show_numbers {
                        output.push(format_numbered_line(line_num, width, line, false));
                    } else {
                        output.push(line.to_string());
                    }
                }

                Ok(CallToolResult::success(vec![Content::text(
                    output.join("\n"),
                )]))
            }
            (Some(_), Some(_)) => Ok(CallToolResult::error(vec![Content::text(
                "Provide either name (buffer) or file_path (file), not both".to_string(),
            )])),
            (None, None) => Ok(CallToolResult::error(vec![Content::text(
                "Provide either name (buffer read) or file_path (file read)".to_string(),
            )])),
        }
    }

    #[tool(description = "List all buffer names, their types, and sizes")]
    async fn workslate_list(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let buffers = self.buffers.read().await;
        if buffers.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No buffers")]));
        }
        let mut lines: Vec<String> = buffers
            .iter()
            .map(|(name, buf)| match buf {
                buf => {
                    let fp = buf.file_path.as_deref().unwrap_or("(no target)");
                    format!("{}: {} lines → {}", name, buf.content.lines().count(), fp)
                }
            })
            .collect();
        lines.sort();
        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }

    #[tool(
        description = "Show unified diff of buffer content against file on disk. file_path falls back to stored target. old_string for partial diff. summary=true for one-line stats."
    )]
    async fn workslate_diff(
        &self,
        Parameters(params): Parameters<DiffParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let buffers = self.buffers.read().await;
        let buffer = match buffers.get(&params.name) {
            Some(b) => b.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Buffer '{}' not found",
                    params.name
                ))]));
            }
        };
        drop(buffers);

        let file_path = match params.file_path.or(buffer.file_path) {
            Some(fp) => fp,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "file_path required (buffer has no stored target)".to_string(),
                )]));
            }
        };

        match tokio::fs::read_to_string(&file_path).await {
            Ok(file_content) => {
                let old_text = if let Some(ref old_string) = params.old_string {
                    let matches: Vec<_> = file_content.match_indices(old_string.as_str()).collect();
                    if matches.is_empty() {
                        return Ok(CallToolResult::error(vec![Content::text(
                            "old_string not found in file".to_string(),
                        )]));
                    }
                    if matches.len() > 1 {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "old_string appears {} times in file (must be unique)",
                            matches.len()
                        ))]));
                    }
                    old_string.clone()
                } else {
                    file_content.clone()
                };

                if params.summary.unwrap_or(false) {
                    let diff = TextDiff::from_lines(&old_text, &buffer.content);
                    let mut hunks = 0u32;
                    let mut adds = 0usize;
                    let mut dels = 0usize;
                    for group in diff.grouped_ops(3) {
                        hunks += 1;
                        for op in &group {
                            match op {
                                DiffOp::Insert { new_index: _, new_len, .. } => adds += new_len,
                                DiffOp::Delete { old_index: _, old_len, .. } => dels += old_len,
                                DiffOp::Replace { old_index: _, old_len, new_index: _, new_len, .. } => {
                                    dels += old_len;
                                    adds += new_len;
                                }
                                _ => {}
                            }
                        }
                    }
                    return Ok(CallToolResult::success(vec![Content::text(
                        format!("{} hunk(s), +{}/-{} lines (target: {})", hunks, adds, dels, file_path)
                    )]));
                }

                let diff = TextDiff::from_lines(&old_text, &buffer.content);
                let unified = diff
                    .unified_diff()
                    .context_radius(3)
                    .header(
                        &format!("a/{}", file_path),
                        &format!("b/{}", file_path),
                    )
                    .to_string();

                if unified.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text(
                        "No differences",
                    )]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(unified)]))
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if params.summary.unwrap_or(false) {
                    let line_count = buffer.content.lines().count();
                    return Ok(CallToolResult::success(vec![Content::text(
                        format!("new file, {} lines (target: {})", line_count, file_path)
                    )]));
                }

                let line_count = buffer.content.lines().count();
                let width = line_count.max(1).to_string().len();
                let numbered: Vec<String> = buffer
                    .content
                    .lines()
                    .enumerate()
                    .map(|(i, line)| format_numbered_line(i + 1, width, line, false))
                    .collect();
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "(new file: {})\n{}",
                    file_path,
                    numbered.join("\n")
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read '{}': {}",
                file_path, e
            ))])),
        }
    }

    #[tool(
        description = "Apply buffer content to file. file_path falls back to stored target. old_string for partial replacement. dry_run=true to preview. Respects buffer depends_on ordering."
    )]
    async fn workslate_apply(
        &self,
        Parameters(params): Parameters<ApplyParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let buffers = self.buffers.read().await;
        let buffer = match buffers.get(&params.name) {
            Some(b) => b.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Buffer '{}' not found",
                    params.name
                ))]));
            }
        };
        drop(buffers);

        if !buffer.depends_on.is_empty() {
            let applied = self.applied_buffers.read().await;
            let unapplied: Vec<&String> = buffer.depends_on.iter()
                .filter(|dep| !applied.contains(*dep))
                .collect();
            if !unapplied.is_empty() {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Cannot apply '{}': unapplied dependencies: {}",
                    params.name, unapplied.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                ))]));
            }
        }

        let file_path = match params.file_path.or(buffer.file_path) {
            Some(fp) => fp,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "file_path required (buffer has no stored target)".to_string(),
                )]));
            }
        };

        if let Some(ref old_string) = params.old_string {
            let file_content = match tokio::fs::read_to_string(&file_path).await {
                Ok(c) => c,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to read '{}': {}",
                        file_path, e
                    ))]));
                }
            };

            let matches: Vec<_> =
                file_content.match_indices(old_string.as_str()).collect();
            if matches.is_empty() {
                return Ok(CallToolResult::error(vec![Content::text(
                    "old_string not found in file".to_string(),
                )]));
            }
            if matches.len() > 1 {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "old_string appears {} times in file (must be unique)",
                    matches.len()
                ))]));
            }

            let final_content = file_content.replacen(old_string.as_str(), &buffer.content, 1);

            if params.dry_run.unwrap_or(false) {
                let line_count = final_content.lines().count();
                let width = line_count.max(1).to_string().len();
                let numbered: Vec<String> = final_content.lines().enumerate()
                    .map(|(i, line)| format_numbered_line(i + 1, width, line, false))
                    .collect();
                return Ok(CallToolResult::success(vec![Content::text(
                    format!("Dry run: would write to '{}' ({} lines)\n{}", file_path, line_count, numbered.join("\n"))
                )]));
            }

            if let Err(e) = tokio::fs::write(&file_path, &final_content).await {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to write '{}': {}",
                    file_path, e
                ))]));
            }

            self.applied_buffers.write().await.insert(params.name.clone());

            Ok(CallToolResult::success(vec![Content::text(format!(
                "Applied buffer '{}' to section in '{}'",
                params.name, file_path
            ))]))
        } else {
            let final_content = buffer.content.clone();

            if params.dry_run.unwrap_or(false) {
                let line_count = final_content.lines().count();
                let width = line_count.max(1).to_string().len();
                let numbered: Vec<String> = final_content.lines().enumerate()
                    .map(|(i, line)| format_numbered_line(i + 1, width, line, false))
                    .collect();
                return Ok(CallToolResult::success(vec![Content::text(
                    format!("Dry run: would write to '{}' ({} lines)\n{}", file_path, line_count, numbered.join("\n"))
                )]));
            }

            if let Some(parent) = std::path::Path::new(&file_path).parent() {
                if !parent.exists() {
                    if let Err(e) = tokio::fs::create_dir_all(parent).await {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to create directory '{}': {}",
                            parent.display(),
                            e
                        ))]));
                    }
                }
            }

            if let Err(e) = tokio::fs::write(&file_path, &final_content).await {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to write '{}': {}",
                    file_path, e
                ))]));
            }

            self.applied_buffers.write().await.insert(params.name.clone());

            Ok(CallToolResult::success(vec![Content::text(format!(
                "Wrote buffer '{}' to '{}'",
                params.name, file_path
            ))]))
        }
    }

    #[tool(description = "Clear a specific buffer by name, or all buffers if no name is given")]
    async fn workslate_clear(
        &self,
        Parameters(params): Parameters<ClearParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut buffers = self.buffers.write().await;
        if let Some(ref name) = params.name {
            if buffers.remove(name).is_some() {
                self.applied_buffers.write().await.remove(name);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Buffer '{}' cleared",
                    name
                ))]))
            } else {
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "Buffer '{}' not found",
                    name
                ))]))
            }
        } else {
            let count = buffers.len();
            buffers.clear();
            self.applied_buffers.write().await.clear();
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Cleared {} buffer(s)",
                count
            ))]))
        }
    }

    // ── Search tool ──────────────────────────────────────

    #[tool(description = "Search a file for a pattern and return matches with line numbers. Use the Summary line numbers with workslate_edit's line_start/line_end for precise edits.")]
    async fn workslate_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        const MAX_MATCHES: usize = 50;

        let metadata = match tokio::fs::metadata(&params.file_path).await {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read '{}': {}",
                    params.file_path, e
                ))]));
            }
        };
        if metadata.len() > MAX_FILE_SIZE {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "File too large ({} bytes, max {})",
                metadata.len(),
                MAX_FILE_SIZE
            ))]));
        }

        let content_bytes = match tokio::fs::read(&params.file_path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read '{}': {}",
                    params.file_path, e
                ))]));
            }
        };

        if is_binary(&content_bytes) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "File appears to be binary: {}",
                params.file_path
            ))]));
        }

        let text = String::from_utf8_lossy(&content_bytes);
        let lines: Vec<&str> = text.lines().collect();
        let ctx = params.context.unwrap_or(2) as usize;
        let use_regex = params.regex.unwrap_or(false);

        let matching_indices: Vec<usize> = if use_regex {
            let re = match Regex::new(&params.pattern) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid regex '{}': {}",
                        params.pattern, e
                    ))]));
                }
            };
            lines
                .iter()
                .enumerate()
                .filter(|(_, line)| re.is_match(line))
                .map(|(i, _)| i)
                .collect()
        } else {
            lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.contains(params.pattern.as_str()))
                .map(|(i, _)| i)
                .collect()
        };

        if matching_indices.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No matches for '{}' in {}",
                params.pattern, params.file_path
            ))]));
        }

        let total_matches = matching_indices.len();
        let truncated = total_matches > MAX_MATCHES;
        let shown_indices = &matching_indices[..matching_indices.len().min(MAX_MATCHES)];

        let max_display_line = shown_indices
            .iter()
            .map(|&i| (i + ctx + 1).min(lines.len()))
            .max()
            .unwrap_or(1);
        let width = max_display_line.to_string().len();

        let mut output = Vec::new();
        if truncated {
            output.push(format!(
                "Found {} matches in {} (showing first {})\n",
                total_matches, params.file_path, MAX_MATCHES
            ));
        } else {
            output.push(format!(
                "Found {} match{} in {}\n",
                total_matches,
                if total_matches == 1 { "" } else { "es" },
                params.file_path
            ));
        }

        for (match_num, &idx) in shown_indices.iter().enumerate() {
            let line_1based = idx + 1;
            let ctx_start = idx.saturating_sub(ctx);
            let ctx_end = (idx + ctx + 1).min(lines.len());

            output.push(format!("Match {} (line {}):", match_num + 1, line_1based));
            for i in ctx_start..ctx_end {
                let is_match_line = i == idx;
                output.push(format_numbered_line(i + 1, width, lines[i], is_match_line));
            }
            output.push(String::new());
        }

        let summary_lines: Vec<String> =
            shown_indices.iter().map(|&i| (i + 1).to_string()).collect();
        if summary_lines.len() <= 20 {
            output.push(format!("Summary: lines {}", summary_lines.join(", ")));
        } else {
            let first_10: Vec<&str> = summary_lines[..10].iter().map(|s| s.as_str()).collect();
            let last_3: Vec<&str> = summary_lines[summary_lines.len() - 3..]
                .iter()
                .map(|s| s.as_str())
                .collect();
            output.push(format!(
                "Summary: lines {}, ..., {} ({} matches shown)",
                first_10.join(", "),
                last_3.join(", "),
                shown_indices.len()
            ));
        }

        Ok(CallToolResult::success(vec![Content::text(
            output.join("\n"),
        )]))
    }

    // ── Task tools ────────────────────────────────────────

    #[tool(description = "Create a task. namespace: 'ws' (default) or 'team'. Returns namespaced ID like ws:1 or team:3.")]
    async fn workslate_task_create(
        &self,
        Parameters(params): Parameters<TaskCreateParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await { return Ok(e); }
        let session = self.active_session.read().await.clone().unwrap();
        let ns_str = params.namespace.as_deref().unwrap_or("ws");
        let ns = match Namespace::parse(ns_str) {
            Ok(n) => n,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let deps: Vec<TaskId> = if let Some(ref dep_strs) = params.depends_on {
            let mut parsed = Vec::new();
            for s in dep_strs {
                match TaskId::parse(s) {
                    Ok(tid) => parsed.push(tid),
                    Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
                }
            }
            parsed
        } else {
            vec![]
        };

        let conn = self.db.lock().unwrap();

        for dep in &deps {
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM tasks WHERE session = ? AND namespace = ? AND id = ?",
                rusqlite::params![session, dep.namespace.as_str(), dep.id],
                |row| row.get(0),
            ).unwrap_or(false);
            if !exists {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "depends_on references non-existent task: {}", dep
                ))]));
            }
        }

        conn.execute(
            "INSERT OR IGNORE INTO task_counters (session, namespace, next_id) VALUES (?, ?, 1)",
            rusqlite::params![session, ns.as_str()],
        ).ok();
        let id: u32 = conn.query_row(
            "SELECT next_id FROM task_counters WHERE session = ? AND namespace = ?",
            rusqlite::params![session, ns.as_str()],
            |row| row.get(0),
        ).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let status = if deps.is_empty() { "pending" } else { "blocked" };
        let deps_json = serialize_depends_on(&deps);

        conn.execute(
            "INSERT INTO tasks (session, namespace, id, name, description, status, owner, depends_on) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![session, ns.as_str(), id, params.name, params.description, status, params.owner, deps_json],
        ).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        conn.execute(
            "UPDATE task_counters SET next_id = ? WHERE session = ? AND namespace = ?",
            rusqlite::params![id + 1, session, ns.as_str()],
        ).ok();

        recompute_blocked_status(&conn, &session).ok();

        let task_id = TaskId { namespace: ns, id };
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} created: {}", task_id, params.name
        ))]))
    }

    #[tool(description = "Mark a task as done. ID format: 3, ws:3, or team:3. Automatically unblocks dependent tasks.")]
    async fn workslate_task_done(
        &self,
        Parameters(params): Parameters<TaskDoneParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await { return Ok(e); }
        let session = self.active_session.read().await.clone().unwrap();
        let tid = match TaskId::parse(&params.id) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let conn = self.db.lock().unwrap();
        let affected = conn.execute(
            "UPDATE tasks SET status = 'done', updated_at = datetime('now') WHERE session = ? AND namespace = ? AND id = ?",
            rusqlite::params![session, tid.namespace.as_str(), tid.id],
        ).unwrap_or(0);

        if affected == 0 {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Task {} not found", tid
            ))]));
        }

        let name: String = conn.query_row(
            "SELECT name FROM tasks WHERE session = ? AND namespace = ? AND id = ?",
            rusqlite::params![session, tid.namespace.as_str(), tid.id],
            |row| row.get(0),
        ).unwrap_or_default();

        recompute_blocked_status(&conn, &session).ok();

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} done: {}", tid, name
        ))]))
    }

    #[tool(description = "Update a task's status, description, or owner. ID format: 3, ws:3, or team:3.")]
    async fn workslate_task_update(
        &self,
        Parameters(params): Parameters<TaskUpdateParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await { return Ok(e); }
        let session = self.active_session.read().await.clone().unwrap();
        let tid = match TaskId::parse(&params.id) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        if let Some(ref s) = params.status {
            if TaskStatus::parse(s).is_err() {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid status '{}'. Must be: pending, in_progress, done, blocked", s
                ))]));
            }
        }

        let conn = self.db.lock().unwrap();

        let (cur_status, cur_desc, cur_owner): (String, Option<String>, Option<String>) = match conn.query_row(
            "SELECT status, description, owner FROM tasks WHERE session = ? AND namespace = ? AND id = ?",
            rusqlite::params![session, tid.namespace.as_str(), tid.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ) {
            Ok(vals) => vals,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Task {} not found", tid
                ))]));
            }
            Err(e) => {
                return Err(rmcp::ErrorData::internal_error(e.to_string(), None));
            }
        };

        let new_status = params.status.unwrap_or(cur_status);
        let new_desc = params.description.or(cur_desc);
        let new_owner = params.owner.or(cur_owner);

        conn.execute(
            "UPDATE tasks SET status = ?, description = ?, owner = ?, updated_at = datetime('now') WHERE session = ? AND namespace = ? AND id = ?",
            rusqlite::params![new_status, new_desc, new_owner, session, tid.namespace.as_str(), tid.id],
        ).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        recompute_blocked_status(&conn, &session).ok();

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} updated", tid
        ))]))
    }

    #[tool(description = "List tasks. Optional namespace filter: 'ws', 'team', or omit for all.")]
    async fn workslate_task_list(
        &self,
        Parameters(params): Parameters<TaskListParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await { return Ok(e); }
        let session = self.active_session.read().await.clone().unwrap();

        let conn = self.db.lock().unwrap();
        let tasks = load_tasks(&conn, &session, params.namespace.as_deref())
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        if tasks.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No tasks")]));
        }

        let mut lines = Vec::new();
        for task in &tasks {
            let icon = match task.status {
                TaskStatus::Done => "\u{2713}",
                TaskStatus::InProgress => "\u{2192}",
                TaskStatus::Pending => " ",
                TaskStatus::Blocked => "\u{2298}",
            };
            let owner_str = task.owner.as_ref()
                .map(|o| format!(" (owner: {})", o)).unwrap_or_default();
            let mut line = format!("{} {}. {}{}", icon, task.display_id(), task.name, owner_str);
            if task.status == TaskStatus::InProgress {
                line.push_str("  \u{2190} in_progress");
            }
            if task.status == TaskStatus::Blocked && !task.depends_on.is_empty() {
                let dep_ids: Vec<String> = task.depends_on.iter().map(|d| d.display()).collect();
                line.push_str(&format!("  (blocked by: {})", dep_ids.join(", ")));
            }
            if let Some(ref desc) = task.description {
                line.push_str(&format!("\n    {}", desc));
            }
            lines.push(line);
        }

        Ok(CallToolResult::success(vec![Content::text(lines.join("\n"))]))
    }

    #[tool(description = "Clear tasks. Optional namespace: 'ws', 'team', or omit to clear all.")]
    async fn workslate_task_clear(
        &self,
        Parameters(params): Parameters<TaskClearParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await { return Ok(e); }
        let session = self.active_session.read().await.clone().unwrap();

        let conn = self.db.lock().unwrap();
        let count: u32 = if let Some(ref ns) = params.namespace {
            let c = conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE session = ? AND namespace = ?",
                rusqlite::params![session, ns], |row| row.get(0),
            ).unwrap_or(0);
            conn.execute(
                "DELETE FROM tasks WHERE session = ? AND namespace = ?",
                rusqlite::params![session, ns],
            ).ok();
            conn.execute(
                "UPDATE task_counters SET next_id = 1 WHERE session = ? AND namespace = ?",
                rusqlite::params![session, ns],
            ).ok();
            c
        } else {
            let c = conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE session = ?",
                rusqlite::params![session], |row| row.get(0),
            ).unwrap_or(0);
            conn.execute("DELETE FROM tasks WHERE session = ?", rusqlite::params![session]).ok();
            conn.execute("DELETE FROM task_counters WHERE session = ?", rusqlite::params![session]).ok();
            c
        };

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Cleared {} task(s)", count
        ))]))
    }

    #[tool(description = "Switch to a named task session. Creates or opens the session in SQLite.")]
    async fn workslate_task_init(
        &self,
        Parameters(params): Parameters<TaskInitParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let json_path = self.tasks_dir.join(format!("tasks-{}.json", params.name));

        let task_count = {
            let conn = self.db.lock().unwrap();

            let existing_count: u32 = conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE session = ?",
                rusqlite::params![params.name],
                |row| row.get(0),
            ).unwrap_or(0);

            if existing_count == 0 {
                if let Ok(json) = std::fs::read_to_string(&json_path) {
                    if let Ok(old_store) = serde_json::from_str::<serde_json::Value>(&json) {
                        if let Some(tasks) = old_store.get("tasks").and_then(|t| t.as_array()) {
                            for task_val in tasks {
                                let id = task_val.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                let name = task_val.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                let desc = task_val.get("description").and_then(|v| v.as_str());
                                let status = task_val.get("status").and_then(|v| v.as_str()).unwrap_or("pending");
                                let deps = task_val.get("depends_on")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        let ids: Vec<TaskId> = arr.iter()
                                            .filter_map(|v| v.as_u64())
                                            .map(|n| TaskId { namespace: Namespace::Ws, id: n as u32 })
                                            .collect();
                                        serialize_depends_on(&ids)
                                    })
                                    .unwrap_or_else(|| "[]".to_string());

                                conn.execute(
                                    "INSERT OR IGNORE INTO tasks (session, namespace, id, name, description, status, depends_on) VALUES (?, 'ws', ?, ?, ?, ?, ?)",
                                    rusqlite::params![params.name, id, name, desc, status, deps],
                                ).ok();
                            }
                            let next_id = old_store.get("next_id").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                            conn.execute(
                                "INSERT OR REPLACE INTO task_counters (session, namespace, next_id) VALUES (?, 'ws', ?)",
                                rusqlite::params![params.name, next_id],
                            ).ok();
                            tracing::info!("Migrated session '{}' from JSON to SQLite", params.name);
                        }
                    }
                }
            }

            conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE session = ?",
                rusqlite::params![params.name],
                |row| row.get(0),
            ).unwrap_or(0u32)
        };

        *self.active_session.write().await = Some(params.name.clone());

        let msg = if task_count > 0 {
            format!("Switched to session '{}' ({} tasks)", params.name, task_count)
        } else {
            format!("Created new session '{}'", params.name)
        };
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(description = "List all available task sessions")]
    async fn workslate_task_sessions(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let active = self.active_session.read().await.clone();
        let conn = self.db.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT session, namespace, COUNT(*), SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) \
             FROM tasks GROUP BY session, namespace ORDER BY session, namespace"
        ).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let rows: Vec<(String, String, u32, u32)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        }).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .filter_map(|r| r.ok())
        .collect();

        if rows.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No sessions")]));
        }

        let mut sessions: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        for (session, ns, total, done) in &rows {
            sessions.entry(session.clone()).or_default()
                .push(format!("{}:[{}/{}]", ns, done, total));
        }

        let mut lines = Vec::new();
        for (session, counters) in &sessions {
            let is_active = active.as_ref().map(|a| a == session).unwrap_or(false);
            let marker = if is_active { " \u{2190} active" } else { "" };
            lines.push(format!("  {} {}{}", session, counters.join(" "), marker));
        }

        Ok(CallToolResult::success(vec![Content::text(lines.join("\n"))]))
    }
}

// ── Task helpers ──────────────────────────────────────────

impl Workslate {
    async fn require_session(&self) -> Result<(), CallToolResult> {
        let session = self.active_session.read().await;
        if session.is_none() {
            return Err(CallToolResult::error(vec![Content::text(
                "No active task session. Call workslate_task_init(name) first.".to_string(),
            )]));
        }
        Ok(())
    }

    async fn append_task_footer(&self, result: &mut CallToolResult) {
        let session = self.active_session.read().await;
        let session_name = match &*session {
            Some(s) => s.clone(),
            None => return,
        };
        drop(session);

        let conn = self.db.lock().unwrap();
        let tasks = match load_tasks(&conn, &session_name, None) {
            Ok(t) => t,
            Err(_) => return,
        };
        drop(conn);

        if tasks.is_empty() {
            return;
        }
        let footer = render_task_footer(&tasks, &session_name);
        result.content.push(Content::text(footer));
    }
}

// ── ServerHandler (manual, replaces #[tool_handler]) ──────

impl ServerHandler for Workslate {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "In-memory named buffers for drafting code before applying to files. \
             workslate_edit with file_path loads from disk and edits; without file_path edits buffer content. \
             workslate_write stores raw content. workslate_apply writes buffer to disk (supports dry_run and buffer dependencies). \
             workslate_diff supports summary mode for one-line stats. \
             Use workslate_read with file_path to view files with line numbers. \
             Use workslate_search to find patterns and get line numbers for workslate_edit. \
             SQLite-backed task tracking with ws: and team: namespaces. \
             Task status is shown automatically in all tool responses.",
        )
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tool_name = request.name.to_string();
        let tcc = ToolCallContext::new(self, request, context);
        let mut result = self.tool_router.call(tcc).await?;

        if matches!(tool_name.as_str(), "workslate_write" | "workslate_edit") {
            let session = self.active_session.read().await;
            if session.is_none() {
                result.content.push(Content::text(
                    "\n\u{26a0} No task session active. For multi-step work, call workslate_task_init first."
                        .to_string(),
                ));
            }
        }

        self.append_task_footer(&mut result).await;
        Ok(result)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }
}

// ── main ──────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cwd = std::env::current_dir()?;
    let project_path = cwd.to_string_lossy().replace('/', "-");
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    let tasks_dir = home
        .join(".claude")
        .join("projects")
        .join(&project_path)
        .join("workslate");
    tokio::fs::create_dir_all(&tasks_dir).await?;

    let db_path = tasks_dir.join("workslate-tasks.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
    conn.execute_batch(SCHEMA_SQL)?;

    let server = Workslate::new(conn, tasks_dir);
    let transport = rmcp::transport::io::stdio();
    let running = server.serve(transport).await?;
    running.waiting().await?;
    Ok(())
}
