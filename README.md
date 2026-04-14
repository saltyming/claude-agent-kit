# Claude Agent Kit

A battle-tested `CLAUDE.md` for Claude Code, plus two custom MCP servers: `workslate` (staged code editing + SQLite-backed task tracking) and `aside` (cross-family second opinions — wraps OpenAI codex, Google gemini, and GitHub copilot CLIs so Claude can consult another model family mid-session).

> **Honest caveat — user intervention is still required.** These rules reduce common failure modes but do not eliminate them. At least two patterns recur even after repeated rule tightening, and you should expect to correct them manually:
>
> 1. **Silent scope reduction.** Despite `[OVERRIDE]`s that explicitly forbid follow-up-PR deferral, stubs, TODOs, and "for now" implementations (see v8.5.1 / v8.5.2 in the Version History), Claude still occasionally splits requested scope at completion time — announcing the split as if it's acceptable, or quietly omitting parts of the spec. Review completion reports critically; push back on any deferred work.
> 2. **Skipping workslate / aside.** The staging workflow and the `advisor()`-paired cross-family second opinion are rules, not automation. Claude routinely falls back to direct `Edit` on files that should be staged through workslate, or calls `advisor()` without firing the paired aside call despite `policy: proactive` (see v8.6.2 for the latest tightening of the pair rule). When you spot the miss, name it out loud — the override exists for exactly that correction.
>
> Treat this kit as a strong prior, not a guarantee. Each observed failure mode has produced a version bump; new phrasings of the same weasel still emerge. If you find a fresh one, file an issue or PR.

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
- **Agent Teams workflow** — self-claim policy, leader intervention patterns, teammate communication triggers, **token cost criteria** (when a team is actually worth it vs. single session), and a **HARD RULE completion report format** that caps per-report tokens
- **Code Staging via workslate** — staged editing workflow that prevents chain-of-thought leakage and scope reduction, with safety rules for `workslate_clear`, stale-buffer handling, and buffer naming across solo/leader/teammate contexts
- **Unified task system** — all contexts (solo, leader, teammate) use `workslate_task_*` with `ws:`/`team:` namespaces, sharing a single SQLite DB via WAL concurrency
- **Quality guardrails** — false claims mitigation, comment discipline, verification requirement before claiming completion

### Workslate MCP Server

An MCP server for Claude Code that provides:

- **Staged code editing** — write code to buffers, review the diff, then apply. Catches mistakes before they reach files. New files show full content with line numbers for review. Buffers persist across server restarts (SQLite-backed).
- **Stale buffer detection** — when a buffer is loaded from disk, workslate records a SHA-256 of the file. At apply time it re-hashes the file and refuses to write if the disk content changed since load. `force=true` overrides. This catches silent data loss when another process (teammate, formatter, user) edited the file behind workslate's back.
- **One buffer per file** — the server rejects creating a second buffer targeting the same file path, forcing you to either extend the existing buffer or explicitly clear it. Prevents conflicting edits from different buffers.
- **Auto-clear on apply** — successful `workslate_apply` removes the buffer from both memory and SQLite automatically; failed apply preserves it for retry. `workslate_clear` is only needed to abandon a buffer you decided not to apply.
- **Safe clear** — `workslate_clear()` without arguments is rejected. You must pass either `name="<buffer>"` or the explicit `all=true` opt-in (which lists the buffers being cleared as a last checkpoint). Guards against catastrophic wipes in shared/team staging areas.
- **File reading with line numbers** — read files from disk with numbered output, feeding directly into line-range editing. Supports range reads (`start_line`/`end_line`).
- **Pattern search** — find patterns (substring or regex) in files, returns matches with context and a summary of line numbers for precise `workslate_edit` targeting.
- **SQLite-backed task tracking** — project-scoped tasks stored in `workslate.db` with WAL mode for concurrent access by multiple agents. Supports `ws:` (personal) and `team:` (coordination) namespaces with cross-namespace dependencies.
- **Named task sessions** — `workslate_task_init("auth-refactor")` isolates tasks per work context. Multiple sessions coexist in SQLite, resumable across restarts.
- **Auto-footer** — every tool response appends a footer showing active session, task progress by namespace (`ws:[3/5] team:[1/3]`), and a `── Buffers: N staged (names) ──` line when any buffers are live. You never lose sight of what's done, what's next, or what's left in staging.
- **Project root guard** — all file operations are restricted to the current working directory tree. The server refuses to read or write outside the project root, even via symlinks.

#### Tools

**Buffer operations:**

| Tool | Description |
|------|-------------|
| `workslate_write(name, content, file_path?, depends_on?)` | Store content in a buffer. If `file_path` given, returns diff for review and records a `source_hash` of the current disk file for stale detection. New files show full content with line numbers. `depends_on` declares buffer application ordering. One buffer per file enforced. |
| `workslate_edit(name, file_path?, old_string?, new_string, position?, match_index?, line_start?, line_end?)` | Stage an edit. With `file_path`: loads file from disk, records `source_hash`, edits. Without: edits existing buffer content (chain edits on a stable buffer). Position: `replace` (default) / `after` / `before` / `append`. Targeting: unique match, `match_index` (Nth occurrence), or `line_start`/`line_end` (line range). One buffer per file enforced. |
| `workslate_read(name?, file_path?, line_numbers?, start_line?, end_line?)` | Read a buffer by name, or read a file from disk with line numbers. File mode supports range reads. |
| `workslate_search(file_path, pattern, regex?, context?)` | Search a file for a pattern. Returns matches with context lines and a Summary of line numbers for use with `workslate_edit`. |
| `workslate_list()` | List all buffers with types and sizes. |
| `workslate_diff(name, file_path?, summary?, old_string?)` | Re-check diff between buffer and file. `summary=true` returns one-line stats (e.g. "2 hunks, +15/-8 lines"). |
| `workslate_apply(name, file_path?, dry_run?, force?, old_string?)` | Apply buffer to file. `dry_run=true` previews without writing (buffer preserved). `force=true` overrides stale buffer detection (disk file changed since load). On successful write, the buffer is automatically cleared from memory and SQLite — no follow-up `workslate_clear` needed. Respects `depends_on` ordering. |
| `workslate_clear(name?, all?)` | Clear a buffer. Pass `name` to clear a specific buffer. To clear ALL staged buffers, pass `all=true` explicitly — bare calls are rejected to prevent accidental wipes. Only needed to abandon a buffer you decided not to apply (successful apply auto-clears). |

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

**Parameter type notes.** Array fields (`depends_on`), boolean fields (`dry_run`,
`force`, `summary`, `regex`, `line_numbers`, `all`) and integer fields
(`match_index`, `line_start`, `line_end`, `start_line`, `end_line`, `context`)
expect native JSON types — arrays, booleans, and numbers respectively.
Stringified JSON values (e.g. `"[\"ws:1\"]"`, `"true"`, `"3"`) are tolerated
as a best-effort shim, but the error message on failure points back at the
expected JSON shape — so always prefer raw JSON values.

### Aside MCP Server

Cross-family second opinions via locally-installed third-party CLIs. Complements — never replaces — the built-in `advisor()` tool (which forwards the transcript to a stronger Claude reviewer). Use `aside` when you want a perspective from a *different* model family: OpenAI codex, Google gemini, or GitHub copilot.

- **Transcript forwarding is on by default.** `include_transcript` defaults to `true`, mirroring the built-in `advisor()` shape. The current Claude Code conversation (parsed from `~/.claude/projects/{dashed-cwd}/<session>.jsonl`) is rendered as plain text and forwarded to the backend, subject to a 100 KB cap (front-trimmed, with a `[transcript truncated: kept last K of M messages]` header when trimming occurs). Pass `include_transcript=false` for decontextualised questions.
- **Three adapters, same schema.** `aside_codex` / `aside_gemini` / `aside_copilot` share the same params (`question`, `context?`, `include_transcript?`, `transcript_tail?`, `timeout_secs?`, `model?`, `reasoning_effort?`). `aside_list` reports which CLIs are on `$PATH` with `--version` strings.
- **Read-only, non-interactive invocation.** Each backend is spawned with flags that prevent file edits or shell execution so the CLI behaves as pure Q&A: codex `-s read-only -a never exec`; gemini `-p ... --approval-mode plan -o text`; copilot `-p ... --allow-all-tools --available-tools= -s --no-color`.
- **Preference-driven call policy.** The install flow generates `~/.claude/rules/claude-agent-kit--aside-prefs.md` where you set a preferred backend, per-backend default models, per-backend reasoning effort, and an auto-call policy (`conservative` / `preference-only` / `proactive`). Claude reads this rule and applies your preferences when the current turn doesn't name a backend or model explicitly. Re-run `make configure` anytime to regenerate it.
- **Custom rules passthrough.** At install time the installer can also copy a directory of your own `*.md` rule files into `~/.claude/rules/`. They get the `claude-agent-kit-custom:user` signature so `make uninstall` preserves them by default (it asks interactively, with an explicit `y` required to remove).
- **Cost awareness.** Every aside call consumes your third-party API quota with the backend provider. See `claude-agent-kit--aside.md` for the usage rules Claude follows (single question per call, no speculative calls, no loops).

#### Tools

| Tool | Description |
|------|-------------|
| `aside_list()` | Report which of codex / gemini / copilot are on `$PATH` and their `--version` output. |
| `aside_codex(question, context?, include_transcript?, transcript_tail?, timeout_secs?, model?, reasoning_effort?)` | Ask OpenAI codex. Maps `model` → `-m`, `reasoning_effort` → `-c model_reasoning_effort=...`. |
| `aside_gemini(question, context?, include_transcript?, transcript_tail?, timeout_secs?, model?, reasoning_effort?)` | Ask Google gemini. Maps `model` → `-m`. `reasoning_effort` is accepted for API symmetry but the gemini CLI currently exposes no flag that consumes it. |
| `aside_copilot(question, context?, include_transcript?, transcript_tail?, timeout_secs?, model?, reasoning_effort?)` | Ask GitHub copilot (standalone CLI, not the `gh` extension). Maps `model` → `--model`, `reasoning_effort` → `--effort` (`low` / `medium` / `high` / `xhigh`). |

#### Required CLIs

You install these separately — `aside` just wraps them:

- [codex](https://github.com/openai/codex) (`npm i -g @openai/codex`)
- [gemini](https://github.com/google-gemini/gemini-cli) (`npm i -g @google/gemini-cli`)
- [copilot](https://docs.github.com/copilot/how-tos/copilot-cli) (GitHub's standalone Copilot CLI — not `gh copilot`)

`aside_list` will tell you which ones this machine has. Missing CLIs are reported as unavailable, not as errors.

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

Downloads the pre-built `workslate` and `aside` binaries from GitHub Releases, `CLAUDE.md`, and rule files. No Rust toolchain required. On macOS, the installer automatically re-signs binaries with `codesign` to prevent endpoint security software (e.g. Kaspersky) from blocking them. The installer registers both MCP servers with Claude Code (if `claude` CLI is available) and then runs an interactive configuration step for `aside` — you'll be prompted for preferred backend / default models / reasoning effort / auto-call policy, and optionally a directory of your own custom rule files to install alongside. All prompts accept ENTER for the documented default, and `ASIDE_*` env vars skip them entirely (useful for CI / automation). If `~/.local/bin` is not in your PATH, the installer will print instructions to add it.

### From source (requires Rust)

```bash
git clone https://github.com/saltyming/claude-agent-kit
cd claude-agent-kit
make install
```

This builds both binaries (`workslate`, `aside`), copies `CLAUDE.md` to `~/.claude/`, rule files to `~/.claude/rules/`, and the binaries to `~/.local/bin/`. On macOS, binaries are re-signed with `codesign` for endpoint security compatibility. A manifest is written to `~/.claude/.claude-agent-kit-manifest` for safe uninstall. The install step ends with an interactive prompt to configure `aside` preferences (see above); re-run anytime with `make configure`.

```bash
make uninstall    # removes kit-owned files; prompts before removing user-owned ones
make configure    # re-run just the aside preferences prompts
```

Uninstall uses signatures on the first line of each `.md` to branch:
- `<!-- claude-agent-kit -->` → kit-owned, removed unconditionally.
- `<!-- claude-agent-kit-custom... -->` → user-owned (the generated `aside-prefs.md` and any ingested custom rules). Preserved by default; you get an interactive `[y/N]` prompt to remove them. Non-interactive runs honor `ASIDE_UNINSTALL_KEEP_PREFS=yes|no`.

The main `CLAUDE.md` contains core principles and quick reference (~125 lines). Detailed rules live in `claude-rules/` (task-execution, parallel-work, git-workflow, framework-conventions, aside) and are auto-loaded by Claude Code from `.claude/rules/`.

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

# Both binaries
cargo build --release -p workslate -p aside
cp target/release/workslate ~/.local/bin/
cp target/release/aside ~/.local/bin/
# macOS: re-sign to avoid endpoint security (Kaspersky, etc.) blocking
codesign --force --sign - ~/.local/bin/workslate
codesign --force --sign - ~/.local/bin/aside

# Register both MCP servers
claude mcp add workslate -s user --transport stdio -- workslate
claude mcp add aside     -s user --transport stdio -- aside
```

## Background

This kit was developed over 2 months of building [SaltyOS](https://github.com/SaltyOS/saltyos), a capability-based microkernel written from scratch in Rust. The project runs 6 parallel Claude Code agents for kernel development, userland servers, and cross-architecture porting. Every rule in the CLAUDE.md exists because something went wrong without it.

Key references that informed the system prompt overrides:

- [Claude Code isn't "stupid now": it's being system prompted to act like that](https://github.com/anthropics/claude-code/issues/30027)
- [Follow-up: Claude Code's source confirms the system prompt problem](https://github.com/anthropics/claude-code/issues/30027)

## License

This work is licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

You are free to share and adapt this material for any purpose, including commercial, as long as you give appropriate credit.
