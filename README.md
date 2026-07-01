# Claude Agent Kit

A battle-tested `CLAUDE.md` for Claude Code, plus three custom MCP servers — `workslate` (staged code editing + SQLite-backed task tracking), `aside` (cross-family second opinions, wrapping the OpenAI codex and GitHub copilot CLIs so Claude can consult another model family mid-session), and `dispatch` (asynchronous hierarchical delegation — handing an execution step to an external coding agent like codex, running write-capable in a target directory). It also ships **palette** — a rules-plus-skills *product-intent outer loop* that wraps the per-task workflow with a durable, cross-session backlog and a slice → build → review cadence (opt in per project via the `palette-init` skill).

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

### palette — product-intent outer loop (rules + skills, no server)

A durable, cross-session planning layer that wraps the per-task workflow. Where the rest of the kit is *within-task* (understand → plan → execute, then it evaporates), palette adds the *outer* loop: a backlog of product intent → slice a thin phase with you → hand a story's acceptance criteria to the normal build workflow → review and re-plan on completion.

- **Opt-in per project, then always-on.** Run the `palette-init` skill to scaffold a `_palette/` directory (a light intake seeds the backlog). Its mere presence turns palette on — the agent consults the backlog automatically. No `_palette/`, no palette: a project that never opted in is untouched (at most a one-line offer on roadmap-shaped work).
- **Advisory, never authoritative.** The backlog / phase / story artifacts *propose* scope; they never authorize an edit. Your existing approval gate authorizes, workslate / dispatch track. A two-way scope firewall keeps palette from shrinking requested work or quietly deferring unmet acceptance criteria at completion time.
- **RST artifacts, robust subset.** Plans live as reStructuredText the agent reads back (backlog, phase briefs, stories) — a deliberately small, grep-friendly subset, not Sphinx-rendered docs. Four **pull-only** skills (`palette-spec` / `palette-ux` / `palette-ui` / `palette-rules`) add tech-spec / UX / design / project-rules depth on demand, never as part of the default loop.
- `_palette/` is a personal planning record — **not committed** by default (`palette-init` offers a `.gitignore`).

Lives in the always-loaded rule `claude-agent-kit--palette.md` plus the `palette-*` skills; no new MCP server. Its plan → build → review methodology is inspired by [mano](https://github.com/ceceppa/mano) (MIT © 2026 ceceppa).

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

### dispatch MCP server

Asynchronous **hierarchical delegation** — hand an execution step to an external coding agent (codex or opencode) running headless and **write-capable**. Where `aside` seeks a read-only opinion, `dispatch` entrusts execution; the run continues in the background and you poll for the result.

- **Async submit → poll / wait → cancel** — `dispatch_submit` returns a task id immediately and runs the backend detached (`codex exec` or a short-lived local `opencode serve`); `dispatch_status` / `dispatch_list` track it, or `dispatch_wait` blocks for you (a **bounded** long-poll — until the task is terminal or a timeout, never an unbounded hold) so you don't busy-poll; `dispatch_cancel` stops a run — or a whole `plan_id` — by killing its process group.
- **Watch + steer** — `dispatch_logs` shows a curated, live timeline of what the backend is doing (codex rollout logs or dispatch-owned OpenCode event JSONL, noise filtered, line-range paged to dodge output limits; default kinds include plaintext OpenCode reasoning but exclude encrypted codex reasoning); `dispatch_steer` interrupts a run and resumes the *same* backend session with a new instruction — its context and the files it already wrote are preserved — as a linked follow-up task. A "watch → redirect" loop, not just fire-and-forget.
- **Structured + free-form task spec** — objective / target_files / constraints / acceptance plus free context/details, rendered deterministically into the backend prompt and stored alongside it for audit.
- **Persistent state** — its own SQLite `dispatch.db`; statuses `queued → running → succeeded / failed / cancelled / interrupted`. Boot reconciliation marks tasks stranded by a dead server `interrupted` without clobbering a peer session's live runs (owner-pid liveness).
- **Server-enforced guards** — working_dir must canonicalize within the project tree (widen with the `DISPATCH_EXTRA_ROOTS` env var); the sandbox ceiling blocks `danger-full-access` unless `DISPATCH_ALLOW_DANGER=1`; one active run per directory unless `allow_concurrent`. Codex uses its CLI sandbox; OpenCode uses OpenCode permission rules plus dispatch's directory guard, not an OS sandbox. Rejections come back as a structured `{error:{code,message}}` so a caller branches on the code rather than parsing prose.
- **Execution policy + approval gate** — `make configure` generates `dispatch-prefs.md` with a `conservative` / `preference-only` / `proactive` execution policy plus a separate approval mode. Because dispatch runs write-capable, `approval mode: ask` still confirms working_dir + step scope + approval granularity before the first submit; `approval mode: auto` pre-authorizes that prompt within server guards. Policy in `claude-agent-kit--dispatch.md`.

Requires a supported backend CLI: [codex](https://github.com/openai/codex) (`npm i -g @openai/codex`) and/or [OpenCode](https://opencode.ai/docs/cli/). `dispatch_backends` reports which are installed.

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

The installer pulls the pre-built `workslate` / `aside` / `dispatch` binaries from GitHub Releases (no Rust needed), installs `CLAUDE.md` + rule files, registers the MCP servers and workslate's doorbell hooks with Claude Code, re-signs binaries on macOS (so endpoint security like Kaspersky doesn't block them), then runs interactive `aside` and `dispatch` config. All prompts accept ENTER for the default; `ASIDE_*` and `DISPATCH_*` env vars skip them for CI.

**From source** (requires Rust):

```bash
git clone https://github.com/saltyming/claude-agent-kit && cd claude-agent-kit
make install      # build + install binaries, CLAUDE.md, rules, hooks; then configure aside + dispatch
make uninstall    # remove kit-owned files (prompts before removing user-owned ones)
make configure    # re-run the aside + dispatch preference prompts
```

Uninstall branches on a first-line signature: `<!-- claude-agent-kit -->` files are removed unconditionally, while `<!-- claude-agent-kit-custom... -->` files (your prefs files and any ingested custom rules) are preserved by default. It also surgically unregisters only workslate's own hooks, leaving any other `settings.json` hooks intact.

**Manual** (no script):

```bash
cp CLAUDE.md ~/.claude/CLAUDE.md && mkdir -p ~/.claude/rules && cp claude-rules/*.md ~/.claude/rules/
cargo build --release -p workslate -p aside -p dispatch && cp target/release/workslate target/release/aside target/release/dispatch ~/.local/bin/
codesign --force --sign - ~/.local/bin/workslate ~/.local/bin/aside ~/.local/bin/dispatch   # macOS only
claude mcp add workslate -s user --transport stdio -- workslate
claude mcp add aside     -s user --transport stdio -- aside
claude mcp add dispatch  -s user --transport stdio -- dispatch
```

The main `CLAUDE.md` is core principles + a quick reference (~125 lines); detailed rules live in `claude-rules/` (task-execution, parallel-work, git-workflow, framework-conventions, aside, dispatch, palette) and auto-load from `.claude/rules/`; the `palette-*` skills install to `.claude/skills/`.

## Background

Developed over months of intensive multi-agent development on a real project — multiple Claude Code agents running in parallel against a shared codebase. Every rule exists because something went wrong without it. Background on the system-prompt overrides: [Claude Code isn't "stupid now": it's being system prompted to act like that](https://github.com/anthropics/claude-code/issues/30027).

## License

[MIT](LICENSE.md) © 2026 Hamin Sung — free to use, modify, and distribute, including commercially; keep the copyright and license notice.
