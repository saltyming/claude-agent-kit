# Claude Agent Kit

A battle-tested `CLAUDE.md` for Claude Code, plus two custom MCP servers — `workslate` (staged code editing + SQLite-backed task tracking) and `aside` (cross-family second opinions, wrapping the OpenAI codex and GitHub copilot CLIs so Claude can consult another model family mid-session).

> **Honest caveat.** These rules reduce common failure modes but don't eliminate them — treat the kit as a strong prior, not a guarantee. Two patterns still recur and need manual correction: **silent scope reduction** (splitting or deferring requested work despite the `[OVERRIDE]`s) and **skipping workslate / aside** (falling back to direct `Edit`, or calling `advisor()` without the paired aside call under `policy: proactive`). Review completion reports critically and name the miss when you see it.

## What's Inside

### CLAUDE.md — system-prompt override manual

Claude Code's stock system prompt is tuned for casual Q&A, not deep engineering. This manual quotes each problematic directive and overrides it:

| System prompt says | What you actually need |
|---|---|
| "Be extra concise. Lead with action, not reasoning." | Explain before acting — the reasoning relevant to the decision, not hidden chain-of-thought. |
| "Only make changes that are directly requested." | Follow the design doc; implement the full scope. |
| "Do not create files unless absolutely necessary." | Create every file the spec calls for. |
| (no verification required) | Verify before claiming completion; never fake a green result. |

It also covers **delegation** (subagents / teammates / the separate `Workflow` tool, with a surface-propose-agree gate and dependency-structure selection), **Agent Teams coordination** (self-claim policy, leader intervention, a token-capped completion-report format), **code staging via workslate**, a **unified `workslate_task_*` task system** (`ws:` / `team:` namespaces), and **quality guardrails** (comment discipline, verification-before-done).

### workslate MCP server

Staged code editing and task tracking:

- **Staged editing** — write to a buffer, review the diff, then apply. New files show full content with line numbers; buffers persist across restarts (SQLite).
- **Stale-buffer detection** — records a SHA-256 at load and refuses to apply if the file changed since (`force=true` overrides).
- **One buffer per file**, **auto-clear on apply**, and **safe clear** (a bare `workslate_clear()` is rejected) — friction that prevents conflicting edits and accidental wipes.
- **File read + pattern search with line numbers** — feeds precise line-range edits.
- **SQLite task tracking** — `ws:` / `team:` namespaces with cross-namespace dependencies, named resumable sessions, WAL concurrency for multiple agents.
- **Doorbell hooks** — a `PostToolUse` footer shows the active session and task progress; a `PreToolUse` inbox nudge delivers role-addressed team messages mid-turn (`workslate_msg_send` → `workslate_inbox_read`), so an Agent Teams leader can steer a teammate before its turn ends. Identity is the composite `(session_id, agent_id)` from the `SessionStart` / `SubagentStart` hooks.
- **Project-root guard** — file operations are confined to the working-directory tree, even via symlinks.

### aside MCP server

Cross-family second opinions via locally-installed CLIs — it complements, never replaces, the built-in `advisor()` (a stronger Claude). Use it for a perspective from a *different* model family: OpenAI codex or GitHub copilot.

- **Transcript auto-forwarded, redacted** — `text` passes through verbatim, but `tool_use` / `tool_result` / `thinking` become placeholders (unlike `advisor()`, which gets the full transcript). 100 KB cap; pass `include_transcript=false` for decontextualised questions.
- **Read-only, non-interactive** — each backend can read files and grep the workspace itself, but cannot edit files or run shells.
- **Preference-driven policy** — `make configure` generates `aside-prefs.md` (preferred backend, default models, reasoning effort, and a `conservative` / `preference-only` / `proactive` auto-call policy). An explicit current-turn instruction to use only one surface ("only aside" / "only `advisor()`") overrides the policy in both directions.
- **Cost-aware** — every call uses your third-party API quota, so the rules cap it to one focused question per call.

Install the CLIs separately (`aside` only wraps them): [codex](https://github.com/openai/codex) (`npm i -g @openai/codex`) and [copilot](https://docs.github.com/copilot/how-tos/copilot-cli) (GitHub's standalone Copilot CLI, not `gh copilot`). `aside_list` reports which are present; missing ones are reported as unavailable, not errors.

## Installation

**macOS / Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.sh | sh
# uninstall (removes only what it installed, verified by signature):
curl -fsSL https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.sh | sh -s -- --uninstall
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1 | iex
# uninstall:
irm https://raw.githubusercontent.com/saltyming/claude-agent-kit/main/install.ps1 -OutFile install.ps1; .\install.ps1 -Uninstall
```

The installer pulls the pre-built `workslate` / `aside` binaries from GitHub Releases (no Rust needed), installs `CLAUDE.md` + rule files, registers both MCP servers and workslate's doorbell hooks with Claude Code, re-signs binaries on macOS (so endpoint security like Kaspersky doesn't block them), then runs an interactive `aside` config (preferred backend, default models, reasoning effort, auto-call policy — and optionally ingests a directory of your own `*.md` rule files alongside). All prompts accept ENTER for the default; `ASIDE_*` env vars skip them for CI.

**From source** (requires Rust):

```bash
git clone https://github.com/saltyming/claude-agent-kit && cd claude-agent-kit
make install      # build + install binaries, CLAUDE.md, rules, hooks; then configure aside
make uninstall    # remove kit-owned files (prompts before removing user-owned ones)
make configure    # re-run just the aside preference prompts
```

Uninstall branches on a first-line signature: `<!-- claude-agent-kit -->` files are removed unconditionally, while `<!-- claude-agent-kit-custom... -->` files (your `aside-prefs.md` and any ingested custom rules) are preserved by default. It also surgically unregisters only workslate's own hooks, leaving any other `settings.json` hooks intact.

**Manual** (no script):

```bash
cp CLAUDE.md ~/.claude/CLAUDE.md && mkdir -p ~/.claude/rules && cp claude-rules/*.md ~/.claude/rules/
cargo build --release -p workslate -p aside && cp target/release/workslate target/release/aside ~/.local/bin/
codesign --force --sign - ~/.local/bin/workslate ~/.local/bin/aside   # macOS only
claude mcp add workslate -s user --transport stdio -- workslate
claude mcp add aside     -s user --transport stdio -- aside
```

The main `CLAUDE.md` is core principles + a quick reference (~125 lines); detailed rules live in `claude-rules/` (task-execution, parallel-work, git-workflow, framework-conventions, aside) and auto-load from `.claude/rules/`.

## Background

Developed over months of intensive multi-agent development on a real project — multiple Claude Code agents running in parallel against a shared codebase. Every rule exists because something went wrong without it. Background on the system-prompt overrides: [Claude Code isn't "stupid now": it's being system prompted to act like that](https://github.com/anthropics/claude-code/issues/30027).

## License

[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) — free to share and adapt for any purpose, including commercial, with appropriate credit.
