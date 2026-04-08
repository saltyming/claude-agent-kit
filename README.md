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
- **Unified task system** — all contexts (solo, leader, teammate) use `workslate_task_*` with `ws:`/`team:` namespaces
- **Quality guardrails** — false claims mitigation, comment discipline, verification fallback

### Workslate MCP Server

An MCP server for Claude Code that provides:

- **Staged code editing** — write code to buffers, review the diff, then apply. Catches mistakes before they reach files. New files show full content with line numbers for review.
- **File reading with line numbers** — read files from disk with numbered output, feeding directly into line-range editing. Supports range reads (`start_line`/`end_line`).
- **Pattern search** — find patterns (substring or regex) in files, returns matches with context and a summary of line numbers for precise `workslate_edit` targeting.
- **SQLite-backed task tracking** — project-scoped tasks stored in `workslate.db` with WAL mode for concurrent access by multiple agents. Supports `ws:` (personal) and `team:` (coordination) namespaces with cross-namespace dependencies.
- **Named task sessions** — `workslate_task_init("auth-refactor")` isolates tasks per work context. Multiple sessions coexist in SQLite, resumable across restarts.
- **Auto-footer** — every tool response includes a task progress summary so you never lose sight of what's done and what's next.

#### Tools

**Buffer operations:**

| Tool | Description |
|------|-------------|
| `workslate_write(name, content, file_path?, depends_on?)` | Store content in a buffer. If `file_path` given, returns diff for review. New files show full content with line numbers. `depends_on` declares buffer application ordering. |
| `workslate_edit(name, file_path?, old_string?, new_string, position?, match_index?, line_start?, line_end?)` | Stage an edit. With `file_path`: loads from disk and edits. Without: edits existing buffer content. Position: `replace`/`after`/`before`/`append`. Targeting: unique match (default), `match_index` (Nth occurrence), or `line_start`/`line_end` (line range). |
| `workslate_read(name?, file_path?, line_numbers?, start_line?, end_line?)` | Read a buffer by name, or read a file from disk with line numbers. File mode supports range reads. |
| `workslate_search(file_path, pattern, regex?, context?)` | Search a file for a pattern. Returns matches with context lines and a Summary of line numbers for use with `workslate_edit`. |
| `workslate_list()` | List all buffers with types and sizes. |
| `workslate_diff(name, file_path?, summary?, old_string?)` | Re-check diff between buffer and file. `summary=true` returns one-line stats (e.g. "2 hunks, +15/-8 lines"). |
| `workslate_apply(name, file_path?, dry_run?, old_string?)` | Apply buffer to file. `dry_run=true` previews without writing. Respects `depends_on` ordering. |
| `workslate_clear(name?)` | Clear one or all buffers. |

**Task tracking:**

| Tool | Description |
|------|-------------|
| `workslate_task_create(name, description?, namespace?, owner?, depends_on?)` | Create a task. `namespace`: `ws` (default) or `team`. `owner` for team task claiming. `depends_on` supports cross-namespace IDs (e.g. `["ws:1", "team:2"]`). |
| `workslate_task_done(id)` | Mark task done. Auto-unblocks dependents. ID format: `"3"`, `"ws:3"`, or `"team:3"`. |
| `workslate_task_update(id, status?, description?, owner?)` | Update task status, description, or owner. |
| `workslate_task_list(namespace?)` | List tasks. Optional namespace filter: `ws`, `team`, or omit for all. |
| `workslate_task_clear(namespace?)` | Clear tasks. Optional namespace filter. |
| `workslate_task_init(name)` | Switch to a named task session (SQLite-backed). |
| `workslate_task_sessions()` | List all sessions with per-namespace counters. |

## Installation

### macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.sh | sh
```

```bash
# Uninstall (only removes files it installed, verified by signature)
curl -fsSL https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.sh | sh -s -- --uninstall
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1 | iex
```

```powershell
# Uninstall
irm https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1 -OutFile install.ps1; .\install.ps1 -Uninstall
```

Downloads the pre-built workslate binary from GitHub Releases, `CLAUDE.md`, and rule files. No Rust toolchain required. On macOS, the installer automatically re-signs the binary with `codesign` to prevent endpoint security software (e.g. Kaspersky) from blocking it. The installer also registers the workslate MCP server with Claude Code (if `claude` CLI is available). If `~/.local/bin` is not in your PATH, the installer will print instructions to add it.

### From source (requires Rust)

```bash
git clone https://github.com/saltyming/claude-agent-kit
cd claude-agent-kit
make install
```

This builds the workslate binary, copies `CLAUDE.md` to `~/.claude/`, rule files to `~/.claude/rules/`, and the binary to `~/.local/bin/`. On macOS, the binary is re-signed with `codesign` for endpoint security compatibility. A manifest is written to `~/.claude/.claude-agent-kit-manifest` for safe uninstall.

```bash
make uninstall    # only removes files it installed (verified by signature)
```

The main `CLAUDE.md` contains core principles and quick reference (~115 lines). Detailed rules live in `claude-rules/` and are auto-loaded by Claude Code from `.claude/rules/`.

### Manual install

```bash
# CLAUDE.md + rules (global)
cp CLAUDE.md ~/.claude/CLAUDE.md
mkdir -p ~/.claude/rules
cp claude-rules/*.md ~/.claude/rules/

# Or project-level
cp CLAUDE.md your-project/CLAUDE.md
mkdir -p your-project/.claude/rules
cp claude-rules/*.md your-project/.claude/rules/

# Workslate binary
cargo build --release -p workslate
cp target/release/workslate ~/.local/bin/
# macOS: re-sign to avoid endpoint security (Kaspersky, etc.) blocking
codesign --force --sign - ~/.local/bin/workslate
```

## Background

This kit was developed over 2 months of building [SaltyOS](https://github.com/SaltyOS/saltyos), a capability-based microkernel written from scratch in Rust. The project runs 6 parallel Claude Code agents for kernel development, userland servers, and cross-architecture porting. Every rule in the CLAUDE.md exists because something went wrong without it.

Key references that informed the system prompt overrides:

- [Claude Code isn't "stupid now": it's being system prompted to act like that](https://github.com/anthropics/claude-code/issues/30027)
- [Follow-up: Claude Code's source confirms the system prompt problem](https://github.com/anthropics/claude-code/issues/30027)

## License

This work is licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

You are free to share and adapt this material for any purpose, including commercial, as long as you give appropriate credit.
