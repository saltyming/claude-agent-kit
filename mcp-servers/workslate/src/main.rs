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
enum BufferContent {
    Raw(String),
    Edit {
        file_path: String,
        old_string: String,
        new_string: String,
    },
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
    /// The exact text to find in the file (must appear exactly once)
    old_string: String,
    /// The replacement text
    new_string: String,
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
        description = "Stage an edit: store old_string → new_string replacement for a file. Returns the unified diff immediately. Use workslate_apply to apply."
    )]
    async fn workslate_edit(
        &self,
        Parameters(params): Parameters<EditBufferParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let file_content = match tokio::fs::read_to_string(&params.file_path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read '{}': {}",
                    params.file_path, e
                ))]));
            }
        };

        let matches: Vec<_> = file_content.match_indices(&params.old_string).collect();
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

        let diff = TextDiff::from_lines(&params.old_string, &params.new_string);
        let unified = diff
            .unified_diff()
            .context_radius(3)
            .header(
                &format!("a/{}", params.file_path),
                &format!("b/{}", params.file_path),
            )
            .to_string();

        let mut buffers = self.buffers.write().await;
        buffers.insert(
            params.name.clone(),
            BufferContent::Edit {
                file_path: params.file_path,
                old_string: params.old_string,
                new_string: params.new_string,
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
            }) => {
                let text = format!(
                    "[edit] {}\n--- old_string ---\n{}\n--- new_string ---\n{}",
                    file_path, old_string, new_string
                );
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
                BufferContent::Edit { file_path, .. } => {
                    format!("{}: edit → {}", name, file_path)
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

                let matches: Vec<_> = file_content.match_indices(&old_string).collect();
                if matches.is_empty() {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "old_string no longer found in file (file may have changed)".to_string(),
                    )]));
                }

                let diff = TextDiff::from_lines(&old_string, &new_string);
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

                let matches: Vec<_> = file_content.match_indices(&old_string).collect();
                if matches.is_empty() {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "old_string no longer found in file (file may have changed)".to_string(),
                    )]));
                }
                if matches.len() > 1 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "old_string appears {} times in file (must be unique)",
                        matches.len()
                    ))]));
                }

                let result = file_content.replacen(&old_string, &new_string, 1);
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
