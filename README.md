# Claude Agent Kit

A battle-tested `CLAUDE.md` for Claude Code and a custom MCP server (`workslate`) for staged code editing with persistent task tracking.

## What's Inside

### CLAUDE.md — System Prompt Override Manual

Claude Code ships with system prompt directives optimized for casual Q&A — not deep engineering work. This manual quotes each problematic directive and provides an explicit `[OVERRIDE]`:

| System Prompt Says | What You Actually Need |
|---|---|
| "Be extra concise. Lead with action, not reasoning." | Explain before acting. Show your reasoning. |
| "Only make changes that are directly requested." | Follow the design doc. Implement the full scope. |
| "Do not create files unless absolutely necessary." | Create every file the spec calls for. |
| (no verification required) | Verify before claiming completion. Never fake a green result. |

Also includes:
- **Agent Teams workflow** — self-claim policy, leader intervention patterns, teammate communication triggers
- **Code Staging via workslate** — staged editing workflow that prevents chain-of-thought leakage and scope reduction
- **Quality guardrails** — false claims mitigation, comment discipline, verification fallback

### Workslate MCP Server

An MCP server for Claude Code that provides:

- **Staged code editing** — write code to buffers, review the diff, then apply. Catches mistakes before they reach files.
- **Persistent task tracking** — project-scoped tasks that survive across sessions, stored in `~/.claude/projects/<project>/workslate/`.
- **Named task sessions** — `workslate_task_init("auth-refactor")` isolates tasks per work context. Multiple sessions coexist, resumable across restarts.
- **Auto-footer** — every tool response includes a task progress summary so you never lose sight of what's done and what's next.

#### Tools

| Tool | Description |
|------|-------------|
| `workslate_write(name, content, file_path?)` | Store content in a buffer. If `file_path` given, returns diff for review. |
| `workslate_edit(name, file_path, old_string?, new_string, position?)` | Stage an edit. Position modes: `replace` (default), `after`, `before`, `append`. Returns diff immediately. |
| `workslate_read(name)` | Read buffer contents. |
| `workslate_list()` | List all buffers with types and sizes. |
| `workslate_diff(name, file_path?)` | Re-check diff between buffer and file. |
| `workslate_apply(name, file_path?)` | Apply buffer to file. Edit buffers need no args. |
| `workslate_clear(name?)` | Clear one or all buffers. |
| `workslate_task_create(name, description?, depends_on?)` | Create a task with optional dependencies. |
| `workslate_task_done(id)` | Mark task done. Auto-unblocks dependents. |
| `workslate_task_update(id, status?, description?)` | Update task status or description. |
| `workslate_task_list()` | List all tasks with status. |
| `workslate_task_clear()` | Clear all tasks for a fresh start. |
| `workslate_task_init(name)` | Switch to a named task session (`tasks-{name}.json`). |
| `workslate_task_sessions()` | List all available task sessions in this project. |

## Installation

### CLAUDE.md

```bash
# Global (applies to all projects)
cp CLAUDE.md ~/.claude/CLAUDE.md

# Or project-level
cp CLAUDE.md your-project/CLAUDE.md
```

### Workslate MCP Server

#### From GitHub Releases (recommended)

Download the binary for your platform from [Releases](https://github.com/saltyming/claude-agent-kit/releases), then configure Claude Code:

```bash
# Extract
tar xzf workslate-aarch64-apple-darwin.tar.gz

# Move to a location on your PATH
mv workslate ~/.local/bin/
```

Add to your Claude Code MCP settings (`~/.claude/settings.json`):

```json
{
  "mcpServers": {
    "workslate": {
      "command": "workslate"
    }
  }
}
```

#### From source

```bash
cargo install --git https://github.com/saltyming/claude-agent-kit --bin workslate
```

#### From a local clone

```bash
git clone https://github.com/saltyming/claude-agent-kit
cd claude-agent-kit
cargo build --release -p workslate

# Binary is at target/release/workslate
```

For local development, point the MCP config to the binary directly:

```json
{
  "mcpServers": {
    "workslate": {
      "command": "/path/to/claude-agent-kit/target/release/workslate"
    }
  }
}
```

## Background

This kit was developed over 2 months of building [SaltyOS](https://github.com/SaltyOS/saltyos), a capability-based microkernel written from scratch in Rust. The project runs 6 parallel Claude Code agents for kernel development, userland servers, and cross-architecture porting. Every rule in the CLAUDE.md exists because something went wrong without it.

Key references that informed the system prompt overrides:

- [Claude Code isn't "stupid now": it's being system prompted to act like that](https://github.com/anthropics/claude-code/issues/30027)
- [Follow-up: Claude Code's source confirms the system prompt problem](https://github.com/anthropics/claude-code/issues/30027)

## License

This work is licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

You are free to share and adapt this material for any purpose, including commercial, as long as you give appropriate credit.
