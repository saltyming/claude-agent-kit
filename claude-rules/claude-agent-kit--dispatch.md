<!-- slate-agent-kit:common -->
# Dispatch Guidance

Policy for the `dispatch` MCP server: it delegates an **execution** step to a coding-agent backend (codex, opencode, or claude) running headless and **write-capable**, asynchronously (submit → poll/wait → cancel/steer). Where dispatch sits relative to `aside` and the harness-native delegates is stated once in `claude-agent-kit--parallel-work.md`'s taxonomy. Operational mechanics — the tool-by-tool async model, spec fields, log paging, steering, server-enforced guards and their error codes — live in the dispatch server's instructions and tool descriptions; this file owns **GATE-DISPATCH** and the judgment rules around delegating.

## Execution policy (when to initiate dispatch)

Read `claude-agent-kit--dispatch-prefs.md` before choosing dispatch; absent or unclear → `conservative`.

- `conservative` — submit only on an explicit current-turn user request, or direct approval of a proposed dispatch.
- `preference-only` — no auto-submit; when the user asks for execution delegation without naming a surface, prefer dispatch for execution-shaped work.
- `proactive` — for suitable steps the agent **SHOULD** initiate dispatch. With `approval mode: ask`, run the gate below first; with `approval mode: auto`, submit directly within the server's hard guards.

**This overrides the general write-capable delegation gate for dispatch specifically.** `CLAUDE.md` and `claude-agent-kit--parallel-work.md` default write-capable delegation to surface-and-propose; `proactive` + `auto` is dispatch's own, narrower, user-configured gate and takes precedence — no proposal round beyond the one-time confirmation below.

Proactive dispatch is for **execution, not judgment**. Suitable: isolated mechanical edits, long verification/fix loops, large well-scoped repetitive sweeps, independent plan steps with clear target files and acceptance criteria. Not suitable: ambiguous product scope, edits overlapping active local/user changes, work needing tight interactive judgment, anything that cannot be one self-contained spec. A configured `model_fallback` chain reduces transient-failure stranding and justifies somewhat longer steps for already-suitable work — it never loosens the judgment exclusions. An explicit current-turn user instruction ("use dispatch" / "do not dispatch") always wins over prefs.

## GATE-DISPATCH: Approval gate (HARD RULE)

**GATE-DISPATCH — dispatch's own instance of INV-GATE-1.** Before the FIRST dispatch in a session, confirm with the user (unless prefs set `approval mode: auto`): (1) the exact **working_dir** the backend will edit, (2) the **step scope** being delegated, (3) the **granularity** — per-step vs whole-plan batch (one `plan_id`). After that, follow the agreed granularity; a genuinely new working_dir or materially wider scope is a fresh confirmation.

**A `model_fallback` retry is never a fresh-confirmation trigger** — the server's automatic same-task retry on a transient backend error reuses the approved objective/working_dir/scope; only the model changes. Do not re-ask permission per attempt. Likewise the server's automatic restart of an unassociated run (`restart_of`) re-runs the already-approved task — not a new dispatch.

**Invariant precedence.** The kernel invariants outrank this gate: a delegated step inherits every invariant (INV-GATE-3) — full approved scope (INV-SCOPE-1), stop-and-ask on forced deviation (GATE-DEVIATION). The gate authorizes delegation; it never licenses delivering less.

## Writing the spec

`dispatch_submit` takes a structured spec (see its tool description for the field list). Frame ONE self-contained step per dispatch; a step depending on another's output waits for that one to reach `succeeded`. **Carry INV-QUALITY-1 into the spec explicitly**: the backend is an external agent optimizing to make the immediate step pass — put the operating envelope into `constraints` (platforms, harnesses, input classes, callers; "fix the cause, not the symptom") and write `acceptance` against the contract, not the triggering case. A `succeeded` on an envelope-free spec proves only that the backend satisfied itself. For a palette story, `objective`/`acceptance` come from the *approved* acceptance criteria, never the raw Tier-A artifact (`claude-agent-kit--palette.md` § Gate bindings).

Partial state: a fallback retry or auto-restart re-runs the same prompt in the same working_dir without resetting files an earlier attempt wrote (dispatch never destructively cleans a tree) — favor convergent, self-contained objectives when configuring fallback chains.

## Supervising a run

- Report a delegated step done only after `dispatch_status`/`dispatch_wait` shows `succeeded` — and review the captured result; exit 0 is not correctness (INV-VERIFY-1 applies to delegated work too).
- **Silence is inconclusive, never a hang verdict.** A live backend inside a long tool/MCP call (e.g. a multi-minute aside consultation) emits no log events. Judge by `child_process_alive` + `log_last_write_age_seconds` from `dispatch_status`, not by a quiet `dispatch_logs`; do not cancel or steer on silence alone. A fresh codex run that never associates its log is auto-restarted once by the server (`restart_of`/`restarted_as` link the pair).
- Steering (`dispatch_steer`) is turn-granularity: it interrupts and resumes the SAME backend session with your correction, context and files preserved.

### Ending a turn while a dispatch task is still running (HARD RULE)

dispatch has **no push notification** — nothing wakes you when a run finishes. Either keep re-invoking `dispatch_wait` in this turn until it returns a terminal status, or — if ending the turn — arm a follow-up check via the harness's scheduling mechanism where one exists, and otherwise tell the user explicitly that the task is still running and they must ask you to check back. Never end a turn on a non-terminal task with nothing armed and no signal to the user.

**Claude Code binding:** this is unlike a backgrounded `Agent`/`Workflow` call (`run_in_background: true`), which auto-notifies the session on completion — dispatch does not. The scheduling mechanism to arm here is `ScheduleWakeup` (or a `/loop`), with the interval matched to the task's expected duration.

## Cost & cleanup

Each dispatch burns backend quota and runs autonomously — no speculative fan-out; delegate only what the execution policy and gate allow. After a cancel, confirm the task reached `cancelled` via `dispatch_status`.
