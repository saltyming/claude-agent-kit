<!-- claude-agent-kit -->
# Dispatch Guidance

Policy for the `dispatch` MCP server (`dispatch_submit` / `dispatch_status` / `dispatch_wait` / `dispatch_list` / `dispatch_cancel` / `dispatch_logs` / `dispatch_steer` / `dispatch_backends`). dispatch delegates an **execution** step to a coding-agent backend (codex or opencode) running headless and **write-capable** — it edits files in a target directory. This is hierarchical delegation (entrust execution), the complement to `aside` (`claude-agent-kit--aside.md`), which is horizontal consultation (seek a read-only opinion). dispatch is also distinct from the `Agent` / `Workflow` delegation in `claude-agent-kit--parallel-work.md`: those spawn Claude subagents; dispatch hands work to a *different* agent process and tracks it asynchronously.

## Async model

MCP calls are request/response, but a delegated run takes minutes, so dispatch is **submit → poll → cancel** (poll by hand, or block on `dispatch_wait`):
- `dispatch_submit(...)` validates, records the task, fires the backend in the background, and returns a task id (e.g. `d-7`) immediately with status `queued`. It does **not** block on the run.
- `dispatch_status(id)` returns the current status (`queued` → `running` → `succeeded` / `failed` / `cancelled` / `interrupted`) and, when terminal, the captured output.
- `dispatch_wait(id, timeout_ms?)` is a **bounded long-poll**: it blocks until the task is terminal *or* `timeout_ms` elapses (default 30s, hard cap 120s), then returns the task row plus a `timed_out` flag. It is deliberately **not** an unbounded block — a held MCP call would hit the client / harness request timeout — so a multi-minute run times out and you simply re-invoke it. Use it instead of busy-polling `dispatch_status` when the next step needs this one finished.
- `dispatch_list(plan_id?, status?)` enumerates tasks.
- `dispatch_cancel(id | plan_id)` stops a run (kills the backend's process group); cancelling by `plan_id` stops every active step of that plan.
- `dispatch_logs(id, line_start?, line_end?, kinds?, raw?)` shows a **curated, live-updating** timeline of what the backend is doing — read from codex's rollout or dispatch-owned OpenCode event JSONL, with noise filtered out (raw tool-call output is never shown by default; messages/reasoning are never truncated per-field — only the total response is size-capped). Works while the task is still running. Page with `line_start`/`line_end` (1-based; omitted = the tail) so a long session can't blow the output budget; `kinds` filters categories (lifecycle/messages/tools/edits/reasoning/tool_results; by default codex excludes reasoning because it is encrypted, while opencode includes plaintext reasoning; `tool_results` — raw tool-call output, e.g. a full file read — is excluded by default for both backends, request it explicitly via `kinds=["tool_results", ...]`), `raw=true` returns the underlying JSONL. Until the run's log is associated it returns `session_pending: true` with an empty log — the honest "not yet", never another session's log.
- `dispatch_steer(id, instruction)` **interrupts and redirects** a run: it cancels the task if still active, then resumes the SAME backend session with your new instruction — the accumulated context and files already written are preserved when supported. Creates a new linked task (`parent_id` = the steered one) so the turn history shows in `dispatch_list`.
- `dispatch_backends()` reports whether codex and opencode are installed.

After submitting, keep working or poll; do not busy-wait — `dispatch_wait` is the non-busy way to block for a terminal status. Report a delegated step as done only after `dispatch_status` (or `dispatch_wait`) shows `succeeded` — and review the captured result, since `succeeded` means the process exited 0, not that the change is correct.

### Ending a turn while a dispatch task is still running (HARD RULE)

dispatch has **no push notification** — it is a plain MCP stdio subprocess, not integrated with the harness's own background-task notification system. This is unlike a backgrounded `Agent`/`Workflow` call (`run_in_background: true`), which auto-notifies the session on completion. If you submit a dispatch task and end your turn without arming a way to re-check it, nothing wakes you — the task keeps running, but neither you nor the user has any signal when it finishes.

Two sanctioned patterns:
- **Short remaining wait** — the task is expected to finish soon: keep re-invoking `dispatch_wait` in the same turn until it returns a terminal status.
- **Ending the turn is otherwise appropriate** — the task will run longer than fits in this turn: before ending the turn, arm a follow-up check where a scheduling mechanism is available for this session (e.g. `ScheduleWakeup`, interval matched to the task's expected duration). Where no such mechanism is available, tell the user explicitly that the task is still running and that they'll need to ask you to check back — do not silently end the turn as if the task will auto-notify like a backgrounded `Agent`/`Workflow` call. Do not end a turn on a non-terminal task with nothing armed and no signal to the user either way.

## Watching a run + steering it

dispatch is a delegate you can supervise, not just a fire-and-forget job — this is the watch → interrupt → redirect loop:
1. `dispatch_logs(id)` to see, live, what the backend is doing (a curated tail of messages / tool calls / file edits). Page with `line_start`/`line_end` for earlier history; the response's `total_lines` tells you how to page.
2. If it's heading the wrong way, `dispatch_steer(id, "<new instruction>")` — it interrupts the run and continues the **same** backend session with your correction (prior context + partial work intact), as a linked follow-up task you then poll like any other.

Steering is at **turn granularity**: you cannot inject into the backend mid-turn, but you can stop it and resume the session with new direction. A follow-up step that depends on a steer should wait for that steer's task to reach `succeeded`.

## Execution policy (when to initiate dispatch)

Read `claude-agent-kit--dispatch-prefs.md` before choosing dispatch. If the file is absent or unclear, default to `execution policy: conservative`.

- `execution policy: conservative` — only submit dispatch after an explicit current-turn user request for dispatch, or after the user directly approves a proposed dispatch.
- `execution policy: preference-only` — do not auto-submit, but when the user asks for execution delegation without naming a surface, prefer dispatch for execution-shaped work and apply the prefs defaults.
- `execution policy: proactive` — for suitable execution steps, Claude **SHOULD** initiate dispatch. With `approval mode: ask`, initiating dispatch means running the approval gate below before the first submit; with `approval mode: auto`, Claude may submit directly within the server's hard guards.

**This overrides the general write-capable delegation gate for dispatch specifically.** `CLAUDE.md`'s Collaboration section and `claude-agent-kit--parallel-work.md` default write-capable delegation (a `general-purpose` subagent, a teammate, a `Workflow`) to surface-and-propose, then wait for the user's agreement before spawning. `execution policy: proactive` is dispatch's own, narrower gate and takes precedence for dispatch: under `proactive` + `approval mode: auto`, Claude submits a suitable step directly — no separate proposal round, no waiting for agreement beyond the one-time approval-gate confirmation below.

Proactive dispatch is for execution, not judgment. Suitable triggers include isolated mechanical edits, long-running verification/fix loops, large but well-scoped repetitive sweeps, or independent plan steps with clear target files and acceptance criteria. Do not proactive-dispatch when the product scope is ambiguous, the expected edits overlap active local/user edits, the task needs tight interactive judgment, or the task cannot be written as one self-contained structured spec.

**Optional addition, separate from the core fallback feature — evaluate independently:** when a `model_fallback` chain is configured, the risk that a proactive dispatch step is silently stranded by a transient backend hiccup (rate limit, quota, momentary model unavailability) is reduced — that class of failure now retries automatically instead of just landing as `failed`. This is a reliability improvement, not a judgment/quality one, so it justifies being somewhat less conservative about step size/duration for triggers already on the suitable list above (e.g. "large but well-scoped repetitive sweeps" can run longer before transient-failure risk alone would have argued for chunking it). It does NOT loosen any of the judgment-based exclusions above (ambiguous product scope, edits overlapping active local/user edits, tasks needing tight interactive judgment, work that isn't one self-contained structured spec) — a model-fallback chain does nothing to make a wrong-but-successful-looking answer less likely; it only makes a backend-availability hiccup less likely to waste the step.

An explicit current-turn user instruction always wins: "use dispatch", "do not dispatch", "only do it yourself", or equivalent overrides the preference file for that turn.

## Approval gate (HARD RULE)

dispatch runs a write-capable subprocess, so **before the FIRST dispatch in a session you MUST confirm with the user directly** (unless their prefs set `approval mode: auto` — see below). Confirm three things:
1. **working_dir** — the exact directory codex will edit.
2. **Step scope** — which step(s) of the plan are being delegated.
3. **Approval granularity** — per-step (confirm each dispatch) vs batch (approve the whole plan once, then dispatch its steps under one `plan_id`).

Read `claude-agent-kit--dispatch-prefs.md`:
- `approval mode: ask` (default) → run the confirmation above before the first dispatch.
- `approval mode: auto` → the user has pre-authorized; you may skip the interactive confirmation. The server's hard guards still apply.

After the first-dispatch confirmation, follow the agreed granularity for the rest of the session. A genuinely new working_dir or a materially wider scope than agreed is a fresh confirmation.

**Fallback retry is not a fresh-confirmation trigger.** When `model_fallback` is set — either passed to `dispatch_submit` directly, or applied from the default in `claude-agent-kit--dispatch-prefs.md` — an automatic retry of the SAME task against the next model in that chain, triggered by the dispatch server's own detection of a transient backend error (rate limit, quota exceeded, model unavailable, auth/permission), is not a new working_dir, not a materially wider scope, and not a new dispatch for purposes of this gate. It reuses the same task id and the same already-approved objective/working_dir/step scope; only the model changes, and the server does it automatically inside the executor's retry loop once the task is submitted. Do not re-run the approval-gate confirmation for it, and do not ask the user's permission before each fallback attempt.

**[OVERRIDE] precedence.** CLAUDE.md `[OVERRIDE]` directives outrank this approval gate. For example, completing the full approved scope of a delegated task is governed by the scope-integrity overrides; do not treat confirmation friction as a reason to deliver less. The gate is about *getting initial authorization to delegate*, not about second-guessing work the user already approved. A delegated dispatch step inherits the same scope-integrity rules as any delegated child — finish the approved scope, no silent reduction, and stop-and-ask on a forced deviation (`claude-agent-kit--task-execution.md`; the delegated-children rule in `claude-agent-kit--parallel-work.md` > *Delegation*).

## Writing the task

`dispatch_submit` takes a **structured spec plus free-form prose** — fill the structure; it gives the backend a consistent, scoped brief:
- `objective` (required) — the self-contained goal of this one step.
- `working_dir` (required, absolute) — where the backend runs.
- `target_files` — files expected to change.
- `constraints` — hard do/don't rules (e.g. "don't touch the public API", "no new dependencies").
- `acceptance` — how to know it's done (tests to pass, behavior to verify).
- `context` / `details` — free-form background and extra instructions.
- `plan_id` — group a plan's steps so they list / cancel as a unit.
- `backend` / `model` / `reasoning_effort` / `sandbox` — execution knobs (defaults from prefs). OpenCode models are `provider/model`; `reasoning_effort` maps to OpenCode `variant`.
- `model_fallback` — optional ordered list of fallback models, tried in order only on a transient backend error (rate limit, quota, model unavailable, auth/permission); a non-transient failure is never retried. If omitted and `claude-agent-kit--dispatch-prefs.md` sets a default (a comma-separated list), split it on commas, trim whitespace, and pass it as this array. `dispatch_status` reports which model actually produced the result (`final_model`) and which earlier models were tried and discarded (`fallback_history`) when a retry occurred. Not honored by `dispatch_steer` — a steered/resumed session stays on one model.

Frame one self-contained step per dispatch. A step that depends on another's output should wait for that one to reach `succeeded` (poll, then submit the next) — dispatch does not sequence steps for you.

**palette note.** If the step implements a palette story, its `objective` / `acceptance` come from that story's *approved* acceptance criteria (post-approval), not from the raw `story-*.rst` file. The backlog proposes, approval authorizes, and the dispatch spec carries the authorized scope. See `claude-agent-kit--palette.md`.

## Model fallback and partial state

A `model_fallback` retry re-sends the SAME prompt to a fresh backend invocation in the SAME `working_dir` — it does not reset or stash any files the failed attempt already wrote. This matters only for write-capable runs (dispatch, not aside): a model-unavailable / quota / auth failure typically fires before the backend makes any edits (it fails at the first model call), so the common case starts the next attempt on a clean tree; a rate limit is the one failure class that can plausibly fire mid-run, after some edits already landed. dispatch does not attempt any git-stash/reset cleanup between attempts — it would be destructive and dispatch explicitly supports non-git working directories. Favor convergent, self-contained objectives (the existing "Frame one self-contained step per dispatch" guidance) when configuring a fallback chain, since those are the objectives a re-run backend agent can safely continue or redo from partial state.

## Server-enforced guards (what will be rejected)

The server enforces these regardless of how the call is phrased — design submits to satisfy them rather than work around them:
- **working_dir containment** — must be absolute, exist, and canonicalize within the project root, or within a root the user added to `DISPATCH_EXTRA_ROOTS`. Anything else is rejected. Do not try to widen it from the model side; the user sets that env var.
- **Sandbox ceiling** — `workspace-write` (default) and `read-only` are allowed; `danger-full-access` is rejected unless the server runs with `DISPATCH_ALLOW_DANGER=1`.
- **OpenCode sandbox note** — OpenCode uses OpenCode permission rules plus dispatch's directory guard, not the OS-level sandbox used by Codex. Treat it as a backend policy layer, not a hard filesystem sandbox.
- **One active run per working_dir** — a second submit against a directory with a live run is rejected; wait, cancel, or pass `allow_concurrent=true` only when concurrent edits to the same tree are genuinely safe.

Rejections come back as a **structured error** — `{ "error": { "code", "message" } }` with a stable `code` (e.g. `working_dir_outside_root`, `sandbox_forbidden`, `dir_busy`, `session_not_ready`, `no_such_task`, `invalid_params`) — so you can branch on the code rather than parse prose.

## Cost & cleanup

Each dispatch consumes the backend's third-party API quota and runs autonomously. Don't fan out speculative dispatches; delegate the steps allowed by the execution policy and approval gate. If you cancel, confirm the task reached `cancelled` via `dispatch_status`.
