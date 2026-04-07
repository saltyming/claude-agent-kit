use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

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
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use similar::TextDiff;
use tokio::sync::RwLock;

// ── Buffer types ──────────────────────────────────────────

#[derive(Clone)]
enum EditMode {
    Replace,
    After,
    Before,
    Append,
}

#[derive(Clone)]
enum BufferContent {
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

struct ResolvedTarget {
    old_text: String,
    byte_start: usize,
    byte_end: usize,
}

fn resolve_target(
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

fn apply_mode(file_content: &str, target: &ResolvedTarget, new_string: &str, mode: &EditMode) -> String {
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

fn diff_texts(target: &ResolvedTarget, new_string: &str, mode: &EditMode, file_content: &str) -> (String, String) {
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
struct WriteParams {
    /// Name of the buffer
    name: String,
    /// Content to store in the buffer
    content: String,
    /// If provided, show unified diff against this file in the response
    file_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EditBufferParams {
    /// Name of the buffer
    name: String,
    /// Path to the file to edit
    file_path: String,
    /// The exact text to find. Required for replace/after/before (unless line_start is used). Ignored for append.
    old_string: Option<String>,
    /// The replacement or insertion text
    new_string: String,
    /// Position mode: "replace" (default), "after" (insert after old_string), "before" (insert before old_string), "append" (append to end of file)
    position: Option<String>,
    /// Target the Nth occurrence of old_string (1-based). Without this, old_string must appear exactly once.
    match_index: Option<u32>,
    /// Target a line range instead of old_string (1-based). When provided, old_string is ignored.
    line_start: Option<u32>,
    /// End of line range (1-based, inclusive). Defaults to line_start if omitted.
    line_end: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReadParams {
    /// Name of the buffer to read
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DiffParams {
    /// Name of the buffer
    name: String,
    /// Path to the file to diff against. Required for raw buffers, ignored for edit buffers.
    file_path: Option<String>,
    /// If provided, diff only this section of the file against the buffer. Only used with raw buffers.
    old_string: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ApplyParams {
    /// Name of the buffer to apply
    name: String,
    /// Path to the target file. Required for raw buffers, ignored for edit buffers.
    file_path: Option<String>,
    /// If provided, replace only this section. Only used with raw buffers.
    old_string: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ClearParams {
    /// Name of the buffer to clear. If omitted, all buffers are cleared.
    name: Option<String>,
}

// ── Task data structures ──────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TaskStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: u32,
    name: String,
    description: Option<String>,
    status: TaskStatus,
    depends_on: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskStore {
    next_id: u32,
    tasks: Vec<Task>,
}

impl TaskStore {
    fn empty() -> Self {
        Self {
            next_id: 1,
            tasks: vec![],
        }
    }

    fn recompute_blocked_status(&mut self) {
        let done_ids: HashSet<u32> = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .map(|t| t.id)
            .collect();

        for task in &mut self.tasks {
            if task.status == TaskStatus::Done || task.status == TaskStatus::InProgress {
                continue;
            }
            if task.depends_on.is_empty() {
                if task.status == TaskStatus::Blocked {
                    task.status = TaskStatus::Pending;
                }
                continue;
            }
            let all_deps_done = task.depends_on.iter().all(|dep| done_ids.contains(dep));
            if all_deps_done {
                task.status = TaskStatus::Pending;
            } else {
                task.status = TaskStatus::Blocked;
            }
        }
    }
}

// ── Task param structs ────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskCreateParams {
    /// Name/title of the task
    name: String,
    /// Optional description with more detail
    description: Option<String>,
    /// Optional list of task IDs this task depends on
    depends_on: Option<Vec<u32>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskDoneParams {
    /// ID of the task to mark as done
    id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskUpdateParams {
    /// ID of the task to update
    id: u32,
    /// New status: pending, in_progress, done, blocked
    status: Option<String>,
    /// New description
    description: Option<String>,
}

// ── Task session param structs ────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
struct TaskInitParams {
    /// Name of the task session (e.g., "auth-refactor"). Creates or loads tasks-{name}.json.
    name: String,
}

// ── Workslate server ──────────────────────────────────────

#[derive(Clone)]
struct Workslate {
    buffers: Arc<RwLock<HashMap<String, BufferContent>>>,
    task_store: Arc<RwLock<TaskStore>>,
    tasks_dir: PathBuf,
    active_session: Arc<RwLock<Option<String>>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Workslate {
    fn new(tasks_dir: PathBuf, task_store: TaskStore) -> Self {
        Self {
            buffers: Arc::new(RwLock::new(HashMap::new())),
            task_store: Arc::new(RwLock::new(task_store)),
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
                Err(_) => Some(format!("(new file: {})", file_path)),
            }
        } else {
            None
        };

        let mut buffers = self.buffers.write().await;
        buffers.insert(params.name.clone(), BufferContent::Raw(params.content));

        let output = match diff_output {
            Some(diff) => format!("{}\n{}", header, diff),
            None => header,
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Stage an edit. Modes: replace (default), after/before (insert around anchor), append. Targeting: old_string (default, unique), match_index (Nth occurrence), line_start/line_end (line range). Returns unified diff."
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

        let file_content = match tokio::fs::read_to_string(&params.file_path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read '{}': {}",
                    params.file_path, e
                ))]));
            }
        };

        let (diff_old, diff_new) = if matches!(mode, EditMode::Append) {
            diff_texts(
                &ResolvedTarget {
                    old_text: String::new(),
                    byte_start: 0,
                    byte_end: 0,
                },
                &params.new_string,
                &mode,
                &file_content,
            )
        } else {
            let target =
                match resolve_target(&file_content, &old_string, params.match_index, line_range) {
                    Ok(t) => t,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(e)]));
                    }
                };
            diff_texts(&target, &params.new_string, &mode, &file_content)
        };

        let diff = TextDiff::from_lines(&diff_old, &diff_new);
        let unified = diff
            .unified_diff()
            .context_radius(3)
            .header(
                &format!("a/{}", params.file_path),
                &format!("b/{}", params.file_path),
            )
            .to_string();

        let stored_old = if line_range.is_some() {
            diff_old.clone()
        } else {
            old_string
        };

        let mut buffers = self.buffers.write().await;
        buffers.insert(
            params.name.clone(),
            BufferContent::Edit {
                file_path: params.file_path,
                old_string: stored_old,
                new_string: params.new_string,
                mode,
                match_index: params.match_index,
                line_range,
            },
        );

        let output = if unified.is_empty() {
            format!("Edit '{}' staged (no differences)", params.name)
        } else {
            format!("Edit '{}' staged\n{}", params.name, unified)
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Read and return the content of a named buffer")]
    async fn workslate_read(
        &self,
        Parameters(params): Parameters<ReadParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let buffers = self.buffers.read().await;
        match buffers.get(&params.name) {
            Some(BufferContent::Raw(content)) => {
                Ok(CallToolResult::success(vec![Content::text(content.clone())]))
            }
            Some(BufferContent::Edit {
                file_path,
                old_string,
                new_string,
                mode,
                match_index,
                line_range,
            }) => {
                let mode_label = match mode {
                    EditMode::Replace => "edit",
                    EditMode::After => "edit:after",
                    EditMode::Before => "edit:before",
                    EditMode::Append => "edit:append",
                };
                let target_label = if let Some((s, e)) = line_range {
                    format!("@L{}-{}", s, e)
                } else if let Some(n) = match_index {
                    format!("#{}", n)
                } else {
                    String::new()
                };
                let text = if matches!(mode, EditMode::Append) {
                    format!(
                        "[{}] {}\n--- new_string ---\n{}",
                        mode_label, file_path, new_string
                    )
                } else {
                    format!(
                        "[{}{}] {}\n--- old_string ---\n{}\n--- new_string ---\n{}",
                        mode_label, target_label, file_path, old_string, new_string
                    )
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::error(vec![Content::text(format!(
                "Buffer '{}' not found",
                params.name
            ))])),
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
                BufferContent::Raw(content) => {
                    format!("{}: raw, {} lines", name, content.lines().count())
                }
                BufferContent::Edit {
                    file_path,
                    mode,
                    match_index,
                    line_range,
                    ..
                } => {
                    let mode_str = match mode {
                        EditMode::Replace => "edit",
                        EditMode::After => "edit:after",
                        EditMode::Before => "edit:before",
                        EditMode::Append => "edit:append",
                    };
                    let target_str = if let Some((s, e)) = line_range {
                        format!("@L{}-{}", s, e)
                    } else if let Some(n) = match_index {
                        format!("#{}", n)
                    } else {
                        String::new()
                    };
                    format!("{}: {}{} → {}", name, mode_str, target_str, file_path)
                }
            })
            .collect();
        lines.sort();
        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }

    #[tool(
        description = "Show unified diff. For edit buffers, no additional args needed. For raw buffers, file_path is required and old_string is optional."
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

        match buffer {
            BufferContent::Edit {
                file_path,
                old_string,
                new_string,
                mode,
                match_index,
                line_range,
            } => {
                let file_content = match tokio::fs::read_to_string(&file_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to read '{}': {}",
                            file_path, e
                        ))]));
                    }
                };

                let (diff_old, diff_new) = if matches!(mode, EditMode::Append) {
                    diff_texts(
                        &ResolvedTarget {
                            old_text: String::new(),
                            byte_start: 0,
                            byte_end: 0,
                        },
                        &new_string,
                        &mode,
                        &file_content,
                    )
                } else {
                    let target = match resolve_target(
                        &file_content,
                        &old_string,
                        match_index,
                        line_range,
                    ) {
                        Ok(t) => t,
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "{} (file may have changed)",
                                e
                            ))]));
                        }
                    };
                    diff_texts(&target, &new_string, &mode, &file_content)
                };

                let diff = TextDiff::from_lines(&diff_old, &diff_new);
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
            BufferContent::Raw(content) => {
                let file_path = match params.file_path {
                    Some(fp) => fp,
                    None => {
                        return Ok(CallToolResult::error(vec![Content::text(
                            "file_path is required for raw buffers".to_string(),
                        )]));
                    }
                };

                let file_content = match tokio::fs::read_to_string(&file_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to read '{}': {}",
                            file_path, e
                        ))]));
                    }
                };

                let old_text = if let Some(ref old_string) = params.old_string {
                    let matches: Vec<_> = file_content.match_indices(old_string).collect();
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

                let diff = TextDiff::from_lines(&old_text, &content);
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
        }
    }

    #[tool(
        description = "Apply a buffer to a file. Edit buffers need no additional args. Raw buffers require file_path; old_string is optional for partial replacement."
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

        match buffer {
            BufferContent::Edit {
                file_path,
                old_string,
                new_string,
                mode,
                match_index,
                line_range,
            } => {
                let file_content = match tokio::fs::read_to_string(&file_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to read '{}': {}",
                            file_path, e
                        ))]));
                    }
                };

                let result = if matches!(mode, EditMode::Append) {
                    apply_mode(&file_content, &ResolvedTarget {
                        old_text: String::new(),
                        byte_start: 0,
                        byte_end: 0,
                    }, &new_string, &mode)
                } else {
                    let target = match resolve_target(
                        &file_content,
                        &old_string,
                        match_index,
                        line_range,
                    ) {
                        Ok(t) => t,
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "{} (file may have changed)",
                                e
                            ))]));
                        }
                    };
                    apply_mode(&file_content, &target, &new_string, &mode)
                };

                if let Err(e) = tokio::fs::write(&file_path, &result).await {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to write '{}': {}",
                        file_path, e
                    ))]));
                }

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Applied edit '{}' to '{}'",
                    params.name, file_path
                ))]))
            }
            BufferContent::Raw(content) => {
                let file_path = match params.file_path {
                    Some(fp) => fp,
                    None => {
                        return Ok(CallToolResult::error(vec![Content::text(
                            "file_path is required for raw buffers".to_string(),
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

                    let new_content = file_content.replacen(old_string.as_str(), &content, 1);
                    if let Err(e) = tokio::fs::write(&file_path, &new_content).await {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to write '{}': {}",
                            file_path, e
                        ))]));
                    }

                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Applied buffer '{}' to section in '{}'",
                        params.name, file_path
                    ))]))
                } else {
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

                    if let Err(e) = tokio::fs::write(&file_path, &content).await {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Failed to write '{}': {}",
                            file_path, e
                        ))]));
                    }

                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Wrote buffer '{}' to '{}'",
                        params.name, file_path
                    ))]))
                }
            }
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
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Cleared {} buffer(s)",
                count
            ))]))
        }
    }

    // ── Task tools ────────────────────────────────────────

    #[tool(description = "Create a new task. Returns the task ID. Use depends_on to declare dependencies on other task IDs.")]
    async fn workslate_task_create(
        &self,
        Parameters(params): Parameters<TaskCreateParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut store = self.task_store.write().await;
        let deps = params.depends_on.unwrap_or_default();

        let existing_ids: HashSet<u32> = store.tasks.iter().map(|t| t.id).collect();
        for dep_id in &deps {
            if !existing_ids.contains(dep_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "depends_on references non-existent task ID: {}",
                    dep_id
                ))]));
            }
        }

        let id = store.next_id;
        store.next_id += 1;

        let status = if deps.is_empty() {
            TaskStatus::Pending
        } else {
            TaskStatus::Blocked
        };

        store.tasks.push(Task {
            id,
            name: params.name.clone(),
            description: params.description,
            status,
            depends_on: deps,
        });

        store.recompute_blocked_status();
        self.save_tasks(&store).await;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} created: {}",
            id, params.name
        ))]))
    }

    #[tool(description = "Mark a task as done. Automatically unblocks dependent tasks.")]
    async fn workslate_task_done(
        &self,
        Parameters(params): Parameters<TaskDoneParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut store = self.task_store.write().await;
        let task = match store.tasks.iter_mut().find(|t| t.id == params.id) {
            Some(t) => t,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Task {} not found",
                    params.id
                ))]));
            }
        };
        task.status = TaskStatus::Done;
        let name = task.name.clone();

        store.recompute_blocked_status();
        self.save_tasks(&store).await;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} done: {}",
            params.id, name
        ))]))
    }

    #[tool(description = "Update a task's status and/or description. Status must be one of: pending, in_progress, done, blocked.")]
    async fn workslate_task_update(
        &self,
        Parameters(params): Parameters<TaskUpdateParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut store = self.task_store.write().await;
        let task = match store.tasks.iter_mut().find(|t| t.id == params.id) {
            Some(t) => t,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Task {} not found",
                    params.id
                ))]));
            }
        };

        if let Some(ref status_str) = params.status {
            let status = match status_str.as_str() {
                "pending" => TaskStatus::Pending,
                "in_progress" => TaskStatus::InProgress,
                "done" => TaskStatus::Done,
                "blocked" => TaskStatus::Blocked,
                _ => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid status '{}'. Must be: pending, in_progress, done, blocked",
                        status_str
                    ))]));
                }
            };
            task.status = status;
        }
        if let Some(desc) = params.description {
            task.description = Some(desc);
        }
        let name = task.name.clone();

        store.recompute_blocked_status();
        self.save_tasks(&store).await;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} updated: {}",
            params.id, name
        ))]))
    }

    #[tool(description = "List all tasks with their status and dependencies")]
    async fn workslate_task_list(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let store = self.task_store.read().await;
        if store.tasks.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No tasks")]));
        }

        let mut lines = Vec::new();
        for task in &store.tasks {
            let status_icon = match task.status {
                TaskStatus::Done => "✓",
                TaskStatus::InProgress => "→",
                TaskStatus::Pending => " ",
                TaskStatus::Blocked => "⊘",
            };
            let mut line = format!("{} {}. {}", status_icon, task.id, task.name);
            if task.status == TaskStatus::InProgress {
                line.push_str("  ← in_progress");
            }
            if task.status == TaskStatus::Blocked && !task.depends_on.is_empty() {
                let dep_ids: Vec<String> = task.depends_on.iter().map(|d| d.to_string()).collect();
                line.push_str(&format!("  (blocked by: {})", dep_ids.join(", ")));
            }
            if let Some(ref desc) = task.description {
                line.push_str(&format!("\n    {}", desc));
            }
            lines.push(line);
        }

        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }

    #[tool(description = "Clear all tasks in the current session. Use when starting a fresh plan.")]
    async fn workslate_task_clear(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut store = self.task_store.write().await;
        let count = store.tasks.len();
        store.tasks.clear();
        store.next_id = 1;
        self.save_tasks(&store).await;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Cleared {} task(s)",
            count
        ))]))
    }

    #[tool(description = "Switch to a named task session. Creates or loads tasks-{name}.json. Use to isolate tasks per work context.")]
    async fn workslate_task_init(
        &self,
        Parameters(params): Parameters<TaskInitParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let new_path = self.tasks_dir.join(format!("tasks-{}.json", params.name));
        let new_store = match tokio::fs::read_to_string(&new_path).await {
            Ok(json) => serde_json::from_str::<TaskStore>(&json).unwrap_or_else(|_| TaskStore::empty()),
            Err(_) => TaskStore::empty(),
        };

        let task_count = new_store.tasks.len();
        *self.task_store.write().await = new_store;
        *self.active_session.write().await = Some(params.name.clone());

        let msg = if task_count > 0 {
            format!("Switched to session '{}' ({} tasks loaded)", params.name, task_count)
        } else {
            format!("Created new session '{}'", params.name)
        };
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(description = "List all available task sessions in this project")]
    async fn workslate_task_sessions(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut entries = match tokio::fs::read_dir(&self.tasks_dir).await {
            Ok(e) => e,
            Err(_) => {
                return Ok(CallToolResult::success(vec![Content::text(
                    "No sessions",
                )]));
            }
        };

        let active = self.active_session.read().await;
        let mut lines = Vec::new();

        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("tasks") || !name.ends_with(".json") {
                continue;
            }
            let session_name = if name == "tasks.json" {
                "(default)".to_string()
            } else {
                name.strip_prefix("tasks-")
                    .and_then(|s| s.strip_suffix(".json"))
                    .unwrap_or(&name)
                    .to_string()
            };

            let is_active = match *active {
                Some(ref a) => *a == session_name,
                None => session_name == "(default)",
            };
            let marker = if is_active { " ← active" } else { "" };

            let task_count = match tokio::fs::read_to_string(entry.path()).await {
                Ok(json) => serde_json::from_str::<TaskStore>(&json)
                    .map(|s| s.tasks.len())
                    .unwrap_or(0),
                Err(_) => 0,
            };

            lines.push(format!("  {}{} ({} tasks)", session_name, marker, task_count));
        }

        if lines.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No sessions",
            )]));
        }
        lines.sort();
        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }
}

// ── Task helpers ──────────────────────────────────────────

impl Workslate {
    fn tasks_path(&self, session: &Option<String>) -> PathBuf {
        match session {
            Some(name) => self.tasks_dir.join(format!("tasks-{}.json", name)),
            None => self.tasks_dir.join("tasks.json"),
        }
    }

    async fn save_tasks(&self, store: &TaskStore) {
        let session = self.active_session.read().await;
        let path = self.tasks_path(&session);
        let json = match serde_json::to_string_pretty(store) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize tasks: {}", e);
                return;
            }
        };
        if let Err(e) = tokio::fs::write(&path, json).await {
            tracing::error!("Failed to write tasks to {:?}: {}", path, e);
        }
    }

    async fn append_task_footer(&self, result: &mut CallToolResult) {
        let store = self.task_store.read().await;
        if store.tasks.is_empty() {
            return;
        }
        let session = self.active_session.read().await;
        let footer = render_task_footer(&store, &*session);
        result.content.push(Content::text(footer));
    }
}

fn render_task_footer(store: &TaskStore, session: &Option<String>) -> String {
    let total = store.tasks.len();
    let done_count = store
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done)
        .count();

    let mut lines = Vec::new();
    let session_label = match session {
        Some(name) => format!(" ({}) ", name),
        None => " ".to_string(),
    };
    lines.push(format!(
        "── Tasks{}[{}/{}] ──────────────────────────",
        session_label, done_count, total
    ));

    if done_count >= 3 {
        lines.push(format!("  ✓ {} done", done_count));
    } else {
        for task in store.tasks.iter().filter(|t| t.status == TaskStatus::Done) {
            lines.push(format!("  ✓ {}. {}", task.id, task.name));
        }
    }

    let mut remaining_slots = 3;
    for task in store
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::InProgress)
    {
        lines.push(format!("  → {}. {}  ← in_progress", task.id, task.name));
        remaining_slots -= 1;
        if remaining_slots == 0 {
            break;
        }
    }

    let non_done_non_progress: Vec<&Task> = store
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::Blocked)
        .collect();

    let show_count = remaining_slots.min(non_done_non_progress.len());
    for task in non_done_non_progress.iter().take(show_count) {
        let mut line = format!("    {}. {}", task.id, task.name);
        if task.status == TaskStatus::Blocked && !task.depends_on.is_empty() {
            let dep_ids: Vec<String> = task.depends_on.iter().map(|d| d.to_string()).collect();
            line.push_str(&format!("  (blocked by: {})", dep_ids.join(", ")));
        }
        lines.push(line);
    }

    let hidden = non_done_non_progress.len().saturating_sub(show_count);
    if hidden > 0 {
        lines.push(format!("    ... and {} more", hidden));
    }

    lines.push("──────────────────────────────────────────".to_string());
    lines.join("\n")
}

// ── ServerHandler (manual, replaces #[tool_handler]) ──────

impl ServerHandler for Workslate {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "In-memory named buffers for drafting code before applying to files. \
             Use workslate_edit for staged old→new replacements, workslate_write for raw content. \
             Persistent project-scoped task tracking across sessions. \
             Task status is shown automatically in all tool responses.",
        )
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tcc = ToolCallContext::new(self, request, context);
        let mut result = self.tool_router.call(tcc).await?;
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
    let default_tasks_path = tasks_dir.join("tasks.json");

    let task_store = match tokio::fs::read_to_string(&default_tasks_path).await {
        Ok(json) => serde_json::from_str::<TaskStore>(&json).unwrap_or_else(|e| {
            tracing::warn!("Failed to parse tasks.json, starting fresh: {}", e);
            TaskStore::empty()
        }),
        Err(_) => TaskStore::empty(),
    };

    let server = Workslate::new(tasks_dir, task_store);
    let transport = rmcp::transport::io::stdio();
    let running = server.serve(transport).await?;
    running.waiting().await?;
    Ok(())
}
