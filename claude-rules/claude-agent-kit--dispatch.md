<!-- slate-agent-kit:common -->
# Dispatch Guidance

Policy for the `dispatch` MCP server (`dispatch_submit` / `dispatch_status` / `dispatch_wait` / `dispatch_list` / `dispatch_cancel` / `dispatch_logs` / `dispatch_steer` / `dispatch_backends`). dispatch delegates an **execution** step to a coding-agent backend (codex, opencode, or claude) running headless and **write-capable** — it edits files in a target directory, asynchronously. Where dispatch sits relative to `aside` and the harness-native delegates is stated once in `claude-agent-kit--parallel-work.md`'s taxonomy. This file owns **GATE-DISPATCH** (the approval gate below).

## Async model

MCP calls are request/response, but a delegated run takes minutes, so dispatch is **submit → poll → cancel** (poll by hand, or block on `dispatch_wait`):
- `dispatch_submit(...)` validates, records the task, fires the backend in the background, and returns a task id (e.g. `d-7`) immediately with status `queued`. It does **not** block on the run.
- `dispatch_status(id)` returns the current status (`queued` → `running` → `succeeded` / `failed` / `cancelled` / `interrupted`) and, when terminal, the captured output.
- `dispatch_wait(id, timeout_ms?)` is a **bounded long-poll**: it blocks until the task is terminal *or* `timeout_ms` elapses (default 30s, hard cap 120s), then returns the task row plus a `timed_out` flag. It is deliberately **not** an unbounded block — a held MCP call would hit the client / harness request timeout — so a multi-minute run times out and you simply re-invoke it. Use it instead of busy-polling `dispatch_status` when the next step needs this one finished.
- `dispatch_list(plan_id?, status?)` enumerates tasks.
- `dispatch_cancel(id | plan_id)` stops a run (kills the backend's process group); cancelling by `plan_id` stops every active step of that plan.
- `dispatch_logs(id, line_start?, line_end?, kinds?, raw?)` shows a **curated, live-updating** timeline of what the backend is doing — read from codex's rollout, dispatch-owned OpenCode event JSONL, or the Claude session log (claude session ids are pinned at spawn, so the log path is deterministic), with noise filtered out (raw tool-call output is never shown by default; messages/reasoning are never truncated per-field — only the total response is size-capped). Works while the task is still running. Page with `line_start`/`line_end` (1-based; omitted = the tail) so a long session can't blow the output budget; `kinds` filters categories (lifecycle/messages/tools/edits/reasoning/tool_results; codex excludes reasoning by default because it is encrypted, while opencode and claude include plaintext reasoning; `tool_results` — raw tool-call output, e.g. a full file read — is excluded by default for every backend, request it explicitly via `kinds=["tool_results", ...]`), `raw=true` returns the underlying JSONL. Until the run's log is associated it returns `session_pending: true` with an empty log — the honest "not yet", never another session's log.
- `dispatch_steer(id, instruction)` **interrupts and redirects** a run: it cancels the task if still active, then resumes the SAME backend session with your new instruction — the accumulated context and files already written are preserved when supported. Creates a new linked task (`parent_id` = the steered one) so the turn history shows in `dispatch_list`. Inherits the steered task's `allow_concurrent` by default; pass `allow_concurrent` to override the per-working_dir guard for the resumed run (needed to steer a task in a directory that has OTHER concurrent runs; `allow_concurrent=false` re-enforces the guard even if the parent bypassed it).
- `dispatch_backends()` reports whether codex, opencode, and claude are installed, plus this server's containment configuration (`project_root`, `extra_roots`) — the first thing to check when a working_dir is rejected.

After submitting, keep working or poll; do not busy-wait — `dispatch_wait` is the non-busy way to block for a terminal status. Report a delegated step as done only after `dispatch_status` (or `dispatch_wait`) shows `succeeded` — and review the captured result, since `succeeded` means the process exited 0, not that the change is correct.

### Ending a turn while a dispatch task is still running (HARD RULE)

dispatch has **no push notification** — it is a plain MCP stdio subprocess, not integrated with any harness's native background-task notification system. If you submit a dispatch task and end your turn without arming a way to re-check it, nothing wakes you — the task keeps running, but neither you nor the user has any signal when it finishes.

Two sanctioned patterns:
- **Short remaining wait** — the task is expected to finish soon: keep re-invoking `dispatch_wait` in the same turn until it returns a terminal status.
- **Ending the turn is otherwise appropriate** — the task will run longer than fits in this turn: before ending the turn, arm a follow-up check where the active harness has a scheduling/reminder mechanism, interval matched to the task's expected duration. Where no such mechanism is available, tell the user explicitly that the task is still running and that they'll need to ask you to check back — do not silently end the turn as if dispatch will auto-notify. Do not end a turn on a non-terminal task with nothing armed and no signal to the user either way.

**Claude Code binding:** this is unlike a backgrounded `Agent`/`Workflow` call (`run_in_background: true`), which auto-notifies the session on completion — dispatch does not. The scheduling mechanism to arm here is `ScheduleWakeup` (or a `/loop`), with the interval matched to the task's expected duration.

## Watching a run + steering it

dispatch is a delegate you can supervise, not just a fire-and-forget job — this is the watch → interrupt → redirect loop:
1. `dispatch_logs(id)` to see, live, what the backend is doing (a curated tail of messages / tool calls / file edits). Page with `line_start`/`line_end` for earlier history; the response's `total_lines` tells you how to page.
2. If it's heading the wrong way, `dispatch_steer(id, "<new instruction>")` — it interrupts the run and continues the **same** backend session with your correction (prior context + partial work intact), as a linked follow-up task you then poll like any other.

Steering is at **turn granularity**: you cannot inject into the backend mid-turn, but you can stop it and resume the session with new direction. A follow-up step that depends on a steer should wait for that steer's task to reach `succeeded`.

## Execution policy (when to initiate dispatch)

Read `claude-agent-kit--dispatch-prefs.md` before choosing dispatch. If the file is absent or unclear, default to `execution policy: conservative`.

- `execution policy: conservative` — only submit dispatch after an explicit current-turn user request for dispatch, or after the user directly approves a proposed dispatch.
- `execution policy: preference-only` — do not auto-submit, but when the user asks for execution delegation without naming a surface, prefer dispatch for execution-shaped work and apply the prefs defaults.
- `execution policy: proactive` — for suitable execution steps, the agent **SHOULD** initiate dispatch. With `approval mode: ask`, initiating dispatch means running the approval gate below before the first submit; with `approval mode: auto`, the agent may submit directly within the server's hard guards.

**This overrides the general write-capable delegation gate for dispatch specifically.** `CLAUDE.md`'s Collaboration section and `claude-agent-kit--parallel-work.md` default write-capable native delegation to surface-and-propose, then wait for the user's agreement before spawning. `execution policy: proactive` is dispatch's own, narrower gate and takes precedence for dispatch: under `proactive` + `approval mode: auto`, the agent submits a suitable step directly — no separate proposal round, no waiting for agreement beyond the one-time approval-gate confirmation below.

Proactive dispatch is for execution, not judgment. Suitable triggers include isolated mechanical edits, long-running verification/fix loops, large but well-scoped repetitive sweeps, or independent plan steps with clear target files and acceptance criteria. Do not proactive-dispatch when the product scope is ambiguous, the expected edits overlap active local/user edits, the task needs tight interactive judgment, or the task cannot be written as one self-contained structured spec.

**Optional addition, separate from the core fallback feature — evaluate independently:** when a `model_fallback` chain is configured, the risk that a proactive dispatch step is silently stranded by a transient backend hiccup (rate limit, quota, momentary model unavailability) is reduced — that class of failure now retries automatically instead of just landing as `failed`. This is a reliability improvement, not a judgment/quality one, so it justifies being somewhat less conservative about step size/duration for triggers already on the suitable list above (e.g. "large but well-scoped repetitive sweeps" can run longer before transient-failure risk alone would have argued for chunking it). It does NOT loosen any of the judgment-based exclusions above (ambiguous product scope, edits overlapping active local/user edits, tasks needing tight interactive judgment, work that isn't one self-contained structured spec) — a model-fallback chain does nothing to make a wrong-but-successful-looking answer less likely; it only makes a backend-availability hiccup less likely to waste the step.

An explicit current-turn user instruction always wins: "use dispatch", "do not dispatch", "only do it yourself", or equivalent overrides the preference file for that turn.

## GATE-DISPATCH: Approval gate (HARD RULE)

**GATE-DISPATCH — dispatch's own instance of INV-GATE-1.** dispatch runs a write-capable subprocess, so **before the FIRST dispatch in a session you MUST confirm with the user directly** (unless their prefs set `approval mode: auto` — see below). Confirm three things:
1. **working_dir** — the exact directory the backend will edit.
2. **Step scope** — which step(s) of the plan are being delegated.
3. **Approval granularity** — per-step (confirm each dispatch) vs batch (approve the whole plan once, then dispatch its steps under one `plan_id`).

Read `claude-agent-kit--dispatch-prefs.md`:
- `approval mode: ask` (default) → run the confirmation above before the first dispatch.
- `approval mode: auto` → the user has pre-authorized; you may skip the interactive confirmation. The server's hard guards still apply.

After the first-dispatch confirmation, follow the agreed granularity for the rest of the session. A genuinely new working_dir or a materially wider scope than agreed is a fresh confirmation.

**Fallback retry is not a fresh-confirmation trigger.** When `model_fallback` is set — either passed to `dispatch_submit` directly, or applied from the default in `claude-agent-kit--dispatch-prefs.md` — an automatic retry of the SAME task against the next model in that chain, triggered by the dispatch server's own detection of a transient backend error (rate limit, quota exceeded, model unavailable, auth/permission), is not a new working_dir, not a materially wider scope, and not a new dispatch for purposes of this gate. It reuses the same task id and the same already-approved objective/working_dir/step scope; only the model changes, and the server does it automatically inside the executor's retry loop once the task is submitted. Do not re-run the approval-gate confirmation for it, and do not ask the user's permission before each fallback attempt.

**Invariant precedence.** The kernel invariants outrank this approval gate: completing the full approved scope of a delegated task is INV-SCOPE-1's territory — do not treat confirmation friction as a reason to deliver less. The gate is about *getting initial authorization to delegate*, not about second-guessing work the user already approved. A delegated dispatch step inherits every invariant as any delegated child does (INV-GATE-3) — finish the approved scope, no silent reduction, stop-and-ask on a forced deviation (GATE-DEVIATION).

## Writing the task

`dispatch_submit` takes a **structured spec plus free-form prose** — fill the structure; it gives the backend a consistent, scoped brief:
- `objective` (required) — the self-contained goal of this one step.
- `working_dir` (required, absolute) — where the backend runs.
- `target_files` — files expected to change.
- `constraints` — hard do/don't rules (e.g. "don't touch the public API", "no new dependencies").
- `acceptance` — how to know it's done (tests to pass, behavior to verify).
- `context` / `details` — free-form background and extra instructions.
- `plan_id` — group a plan's steps so they list / cancel as a unit.
- `backend` / `model` / `reasoning_effort` / `sandbox` — execution knobs (defaults from prefs). OpenCode models are `provider/model`; `reasoning_effort` maps to OpenCode `variant`. Claude models are aliases or full names (`haiku`, `sonnet`, `opus`, …); `reasoning_effort` maps to `claude --effort`; steering a claude task resumes the same conversation under a new pinned session id (`--resume … --fork-session`).
- `model_fallback` — optional ordered list of fallback models, tried in order only on a transient backend error (rate limit, quota, model unavailable, auth/permission); a non-transient failure is never retried. If omitted and `claude-agent-kit--dispatch-prefs.md` sets a default (a comma-separated list), split it on commas, trim whitespace, and pass it as this array. `dispatch_status` reports which model actually produced the result (`final_model`) and which earlier models were tried and discarded (`fallback_history`) when a retry occurred. Not honored by `dispatch_steer` — a steered/resumed session stays on one model.

Frame one self-contained step per dispatch. A step that depends on another's output should wait for that one to reach `succeeded` (poll, then submit the next) — dispatch does not sequence steps for you.

**Carry INV-QUALITY-1 into the spec explicitly.** The backend behind a dispatch is an external agent that optimizes for making the immediate step pass, and it may not carry this rule corpus at all — left unconstrained it will deliver a short-horizon patch that satisfies the triggering case and nothing else. Put the operating envelope into `constraints` (the platforms, harnesses, input classes, and callers the change must hold under; "fix the cause, not the symptom"), and write `acceptance` against the contract rather than the triggering case (e.g. "path handling also holds for Windows separators", not just "this test passes"). A `succeeded` on a spec without an envelope proves only that the backend satisfied itself.

For a palette story, the spec's `objective` / `acceptance` come from the *approved* acceptance criteria, never the raw Tier-A artifact (`claude-agent-kit--palette.md` § Gate bindings).

## Model fallback and partial state

A `model_fallback` retry re-sends the SAME prompt to a fresh backend invocation in the SAME `working_dir` — it does not reset or stash any files the failed attempt already wrote. This matters only for write-capable runs (dispatch, not aside): a model-unavailable / quota / auth failure typically fires before the backend makes any edits (it fails at the first model call), so the common case starts the next attempt on a clean tree; a rate limit is the one failure class that can plausibly fire mid-run, after some edits already landed. dispatch does not attempt any git-stash/reset cleanup between attempts — it would be destructive and dispatch explicitly supports non-git working directories. Favor convergent, self-contained objectives (the existing "Frame one self-contained step per dispatch" guidance) when configuring a fallback chain, since those are the objectives a re-run backend agent can safely continue or redo from partial state.

## Server-enforced guards (what will be rejected)

The server enforces these regardless of how the call is phrased — design submits to satisfy them rather than work around them:
- **working_dir containment** — must be absolute, exist, and canonicalize within the project root, or within a root the user added to `DISPATCH_EXTRA_ROOTS`. Anything else is rejected. Do not try to widen it from the model side; the user sets that env var (installer flag `--roots`). When the harness spawned the MCP server outside any project (e.g. a plugin runtime), there is no project root at all — submits then fail with `no_project_root` until `DISPATCH_EXTRA_ROOTS` (or `SLATE_PROJECT_DIR`) is configured; `dispatch_backends` shows the live containment configuration.
- **Sandbox ceiling** — `workspace-write` (default) and `read-only` are allowed; `danger-full-access` is rejected unless the server runs with `DISPATCH_ALLOW_DANGER=1`.
- **Backend sandbox notes** — OpenCode uses OpenCode permission rules plus dispatch's directory guard; the claude backend maps the sandbox to Claude Code permission modes (`read-only` → plan, `workspace-write` → acceptEdits with Bash allowed, `danger-full-access` → skip permissions). Both are backend policy layers, not the OS-level sandbox Codex uses.
- **One active run per working_dir** — a second submit against a directory with a live run is rejected; wait, cancel, or pass `allow_concurrent=true` only when concurrent edits to the same tree are genuinely safe. `dispatch_steer` inherits the steered task's `allow_concurrent` by default (persisted on the task) and accepts an explicit `allow_concurrent` override, so a task in a concurrent-run directory can still be steered without hitting `dir_busy`.
- **No re-entrant dispatch** — a `dispatch_submit` / `dispatch_steer` issued from *inside* a dispatch-spawned backend is refused (error `reentrant`) before any enqueue or spawn, blocking `backend → dispatch → backend → …` recursion. Two mechanisms close it per backend: for **claude** and **opencode** (which forward their process env to the MCP servers they boot) the `DISPATCH_REENTRY_DEPTH` env marker, stamped on every spawned backend and inherited down its process tree, makes a nested dispatch see the marker and refuse; **codex** does not propagate process env to its MCP children, so it is guarded fail-closed by spawning it with `-c mcp_servers.dispatch.enabled=false` (the dispatch MCP server is not loaded at all). Either way the guard is scoped to dispatch only: `aside` is left enabled, so a dispatch backend may still consult `aside` (a read-only advisor) — a legitimate cross-surface call, not recursion. Do the follow-up work in the current backend session instead of nesting a dispatch.

Rejections come back as a **structured error** — `{ "error": { "code", "message" } }` with a stable `code` (e.g. `invalid_working_dir`, `no_project_root`, `sandbox_forbidden`, `dir_busy`, `session_not_ready`, `no_such_task`, `invalid_params`, `reentrant`) — so you can branch on the code rather than parse prose.

## Cost & cleanup

Each dispatch consumes the backend's third-party API quota and runs autonomously. Don't fan out speculative dispatches; delegate the steps allowed by the execution policy and approval gate. If you cancel, confirm the task reached `cancelled` via `dispatch_status`.
