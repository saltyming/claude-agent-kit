# Claude Agent Kit

A `CLAUDE.md` and rule set for Claude Code, plus the two shared Slate MCP servers: `aside` (second opinions from another model family) and `dispatch` (asynchronous delegation to codex / opencode / claude backends). Their source lives in [`slate-agent-kit`](https://github.com/saltyming/slate-agent-kit)`/shared/mcp-servers` and is built and registered from there. The kit also ships **palette**, a rules-plus-skills product-intent outer loop that wraps the per-task workflow with a cross-session backlog and a slice, build, review cadence (opt in per project with the `palette-init` skill).

> **Honest caveat.** These rules reduce common failure modes but don't eliminate them — treat the kit as a strong prior, not a guarantee. Two patterns still recur and need manual correction: **silent scope reduction** (splitting or deferring requested work despite INV-SCOPE-1) and **skipping the aside pairing** (calling `advisor()` without the paired aside call under `policy: proactive`). Review completion reports critically and name the miss when you see it.

## What's Inside

### CLAUDE.md and the rule files

The manual states a small set of invariants, each under a stable ID, and the rule files hold the procedures that refer to them:

| Area | What it holds |
|---|---|
| **Scope** (`INV-SCOPE-*`) | Deliver the whole requested scope; the user owns scope; deviate from an approved plan only through a stop-and-ask gate. |
| **State** (`INV-STATE-*`) | No rollback on the agent's own initiative; "undo" reverses this session's edits, not repository state; the user's uncommitted changes are left alone. |
| **Verification** (`INV-VERIFY-*`) | Verify before claiming completion; report failures as failures. |
| **Quality** (`INV-QUALITY-1`) | Write for every platform and caller the code claims to support, and fix the cause, not the symptom. |
| **Delegation** (`INV-GATE-*`) | Read-only delegates are free; write-capable ones are proposed to the user first. |
| **Communication** (`INV-COMM-*`) | Formal register; plain wording without stock metaphors, filler intensifiers, or flattery openers. |

Since 12.0.0 the rules are written as a default action plus the named behaviors to avoid, with the reason, instead of lists of permitted cases, and they leave tool usage to the harness system prompt and each tool's own description. The standing corpus is about 52 KB.

### palette — product-intent outer loop (rules + skills, no server)

A durable, cross-session planning layer that wraps the per-task workflow. Where the rest of the kit is *within-task* (understand → plan → execute, then it evaporates), palette adds the *outer* loop: a backlog of product intent → slice a thin phase with you → hand a story's acceptance criteria to the normal build workflow → review and re-plan on completion.

- **Opt-in per project, then always-on.** Run the `palette-init` skill to scaffold a `_palette/` directory (a light intake seeds the backlog). Its mere presence turns palette on — the agent consults the backlog automatically. No `_palette/`, no palette: a project that never opted in is untouched (at most a one-line offer on roadmap-shaped work).
- **Advisory, never authoritative.** The backlog / phase / story artifacts *propose* scope; they never authorize an edit. Your existing approval gate authorizes. A two-way scope firewall keeps palette from shrinking requested work or quietly deferring unmet acceptance criteria at completion time.
- **RST artifacts, robust subset.** Plans live as reStructuredText the agent reads back (backlog, phase briefs, stories) — a deliberately small, grep-friendly subset, not Sphinx-rendered docs. Four **pull-only** skills (`palette-spec` / `palette-ux` / `palette-ui` / `palette-rules`) add tech-spec / UX / design / project-rules depth on demand, never as part of the default loop.
- `_palette/` is a personal planning record — **not committed** by default (`palette-init` offers a `.gitignore`).

Lives in the always-loaded rule `claude-agent-kit--palette.md` plus the `palette-*` skills; no new MCP server. Its plan → build → review methodology is inspired by [mano](https://github.com/ceceppa/mano) (MIT © 2026 ceceppa).

### aside MCP server

Second opinions via locally-installed CLIs — it complements, never replaces, the built-in `advisor()` (a stronger Claude). Use it for a perspective from a different model family via OpenAI codex or GitHub copilot, or for a separate local Claude CLI pass via `aside_claude`.

- **Transcript auto-forwarded, redacted** — `text` passes through verbatim, but `tool_use` / `tool_result` / `thinking` become placeholders (unlike `advisor()`, which gets the full transcript). 100 KB cap; pass `include_transcript=false` for decontextualised questions.
- **Read-only, non-interactive** — each backend can read files and grep the workspace itself, but cannot edit files or run shells.
- **Preference-driven policy** — `make configure` generates `aside-prefs.md` (preferred backend, default models, reasoning effort, and a `conservative` / `preference-only` / `proactive` auto-call policy). An explicit current-turn instruction to use only one surface ("only aside" / "only `advisor()`") overrides the policy in both directions.
- **Cost-aware** — every call uses your third-party API quota, so the rules cap it to one focused question per call.

Install the CLIs separately (`aside` only wraps them): [codex](https://github.com/openai/codex) (`npm i -g @openai/codex`), [copilot](https://docs.github.com/copilot/how-tos/copilot-cli) (GitHub's standalone Copilot CLI, not `gh copilot`), and [Claude Code](https://claude.com/claude-code) (`npm i -g @anthropic-ai/claude-code`). `aside_list` reports which are present; missing ones are reported as unavailable, not errors.

### dispatch MCP server

Asynchronous **hierarchical delegation** — hand an execution step to an external coding agent (codex, opencode, or claude) running headless and **write-capable**. Where `aside` seeks a read-only opinion, `dispatch` entrusts execution; the run continues in the background and you poll for the result.

- **Async submit → poll → cancel** — `dispatch_submit` returns a task id immediately and runs the backend detached (`codex exec` or a short-lived local `opencode serve`); `dispatch_status` / `dispatch_list` track it and `dispatch_logs` shows the curated timeline; `dispatch_cancel` stops a run — or a whole `plan_id` — by killing its process group.
- **Watch + steer** — `dispatch_logs` shows a curated, live timeline of what the backend is doing (codex rollout logs or dispatch-owned OpenCode event JSONL, noise filtered, line-range paged to dodge output limits; default kinds include plaintext OpenCode reasoning but exclude encrypted codex reasoning); `dispatch_steer` interrupts a run and resumes the *same* backend session with a new instruction — its context and the files it already wrote are preserved — as a linked follow-up task. A "watch → redirect" loop, not just fire-and-forget.
- **Structured + free-form task spec** — objective / target_files / constraints / acceptance plus free context/details, rendered deterministically into the backend prompt and stored alongside it for audit.
- **Persistent state** — its own SQLite `dispatch.db`; statuses `queued → running → succeeded / failed / cancelled / interrupted`. Boot reconciliation marks tasks stranded by a dead server `interrupted` without clobbering a peer session's live runs (owner-pid liveness).
- **Server-enforced guards** — working_dir must canonicalize within the project tree (widen with the `DISPATCH_EXTRA_ROOTS` env var); the sandbox ceiling blocks `danger-full-access` unless `DISPATCH_ALLOW_DANGER=1`; one active run per directory unless `allow_concurrent`. Codex uses its CLI sandbox; OpenCode uses OpenCode permission rules plus dispatch's directory guard, not an OS sandbox. Rejections come back as a structured `{error:{code,message}}` so a caller branches on the code rather than parsing prose.
- **Execution policy + approval gate** — `make configure` generates `dispatch-prefs.md` with a `conservative` / `preference-only` / `proactive` execution policy plus a separate approval mode. Because dispatch runs write-capable, `approval mode: ask` still confirms working_dir + step scope + approval granularity before the first submit; `approval mode: auto` pre-authorizes that prompt within server guards. Policy in `claude-agent-kit--dispatch.md`.

Requires a supported backend CLI: [codex](https://github.com/openai/codex) (`npm i -g @openai/codex`), [OpenCode](https://opencode.ai/docs/cli/), and/or [Claude Code](https://claude.com/claude-code) (`npm i -g @anthropic-ai/claude-code`). `dispatch_backends` reports which are installed.

### Git preferences

Commit signing, model attribution, commit message format, PR body format, and branch naming are yours, not the kit's. They live in the user-owned `~/.claude/rules/claude-agent-kit--git-prefs.md`, which installs with every value `unset`. Before the first commit or PR that needs a value, the agent asks you and writes the answer into that file; when a repository's own convention differs from your preference, it asks which to follow there and records that too. You can edit the file by hand at any time, and upgrades never overwrite it.

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

The installer builds and registers the shared `aside` / `dispatch` from a slate-agent-kit checkout (`SLATE_AGENT_KIT_DIR`, a sibling `../slate-agent-kit`, or a shallow clone; needs Rust; `SKIP_MCP=1` skips), installs `CLAUDE.md` and the rule files, then runs the interactive `aside` and `dispatch` configuration and installs the git preferences file if it is absent. All prompts accept ENTER for the default; `ASIDE_*` and `DISPATCH_*` env vars skip them for CI. Upgrading from 11.x also removes workslate (its `settings.json` hooks, binary, MCP registration, and per-project db); your other hooks are left untouched and `settings.json` is backed up before it is edited.

**From source** (Rust is needed only for the shared MCP servers):

```bash
git clone https://github.com/saltyming/claude-agent-kit && cd claude-agent-kit
make install      # install CLAUDE.md, rules, skills; register aside + dispatch; then configure prefs
make uninstall    # remove kit-owned files (prompts before removing user-owned ones)
make configure    # re-run the aside + dispatch preference prompts
```

Uninstall branches on a first-line signature: kit-managed files carry `<!-- slate-agent-kit:common -->` and are removed, while `<!-- claude-agent-kit-custom... -->` files (your prefs files and any ingested custom rules) are preserved by default.

**Manual** (no script):

```bash
cp CLAUDE.md ~/.claude/CLAUDE.md && mkdir -p ~/.claude/rules && cp claude-rules/*.md ~/.claude/rules/
cp scripts/claude-agent-kit--git-prefs.md.tmpl ~/.claude/rules/claude-agent-kit--git-prefs.md
# aside + dispatch (with their required env — ASIDE_HARNESS=claude,
# SLATE_AGENT_STATE_HOME) are registered from a slate-agent-kit checkout:
#   <slate>/tooling/install-mcp.sh --configure-claude
```

The main `CLAUDE.md` is the invariants, core principles, and a quick reference; detailed rules live in `claude-rules/` (task-execution, parallel-work, git-workflow, framework-conventions, aside, dispatch, palette) and auto-load from `.claude/rules/`; the `palette-*` skills install to `.claude/skills/`.

## Background

Developed over months of intensive multi-agent development on a real project — multiple Claude Code agents running in parallel against a shared codebase. Every rule exists because something went wrong without it. Background on the system-prompt overrides: [Claude Code isn't "stupid now": it's being system prompted to act like that](https://github.com/anthropics/claude-code/issues/30027).

## License

[MIT](LICENSE.md) © 2026 Hamin Sung — free to use, modify, and distribute, including commercially; keep the copyright and license notice.
