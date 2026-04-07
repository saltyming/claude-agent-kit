use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ── Task data structures ──────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub depends_on: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStore {
    pub next_id: u32,
    pub tasks: Vec<Task>,
}

impl TaskStore {
    pub fn empty() -> Self {
        Self {
            next_id: 1,
            tasks: vec![],
        }
    }

    pub fn recompute_blocked_status(&mut self) {
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
pub struct TaskCreateParams {
    /// Name/title of the task
    pub name: String,
    /// Optional description with more detail
    pub description: Option<String>,
    /// Optional list of task IDs this task depends on
    pub depends_on: Option<Vec<u32>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskDoneParams {
    /// ID of the task to mark as done
    pub id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskUpdateParams {
    /// ID of the task to update
    pub id: u32,
    /// New status: pending, in_progress, done, blocked
    pub status: Option<String>,
    /// New description
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskInitParams {
    /// Name of the task session (e.g., "auth-refactor"). Creates or loads tasks-{name}.json.
    pub name: String,
}

// ── Task footer rendering ─────────────────────────────────

pub fn render_task_footer(store: &TaskStore, session: &Option<String>) -> String {
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
