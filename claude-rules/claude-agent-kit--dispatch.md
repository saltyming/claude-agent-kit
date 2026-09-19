<!-- slate-agent-kit:common -->
# Dispatch Guidance

Policy for the `dispatch` MCP server, which hands an execution step to a coding-agent backend that runs headless, write-capable, and asynchronously. How to operate it (the submit, status, logs, steer, and cancel tools, the spec fields, reading a quiet log, the server-enforced guards) is in the server's own instructions and tool descriptions. This file holds GATE-DISPATCH and the judgment around delegating. For how dispatch relates to `aside` and the harness-native delegates, see `claude-agent-kit--parallel-work.md`.

## Execution policy

Read `claude-agent-kit--dispatch-prefs.md` before choosing dispatch. If it is absent or unclear, use `conservative`.

- `conservative`: submit only when the user asks for dispatch in the current turn, or approves a dispatch you proposed.
- `preference-only`: do not auto-submit. When the user asks for execution delegation without naming a surface, prefer dispatch for execution-shaped work.
- `proactive`: initiate dispatch for suitable steps. With `approval mode: ask`, run GATE-DISPATCH first. With `approval mode: auto`, submit directly, within the server's guards.

`proactive` with `auto` is the user-configured policy that INV-GATE-1 allows for a specific mechanism. For dispatch it replaces the propose-and-wait round of GATE-DELEGATE. A current-turn instruction from the user ("use dispatch", "do not dispatch") outranks prefs.

Dispatch is for execution, not judgment. It fits isolated mechanical edits, long verify-and-fix loops, large well-scoped repetitive sweeps, and independent plan steps with clear target files and acceptance criteria. Do not dispatch work with ambiguous product scope, edits that overlap active local or user changes, work that needs close interactive judgment, or anything that cannot be written as one self-contained spec. A configured `model_fallback` chain makes a stranded run less likely and justifies somewhat longer steps for work that already fits. It does not relax these exclusions.

## GATE-DISPATCH: approval gate

**GATE-DISPATCH — dispatch's own instance of INV-GATE-1.** Before the first dispatch in a session, unless prefs set `approval mode: auto`, confirm with the user: the `working_dir` the backend will edit, the step scope being delegated, and the granularity (each step separately, or the whole plan as one batch under one `plan_id`). After that, follow the agreed granularity. A new `working_dir` or a materially wider scope needs a new confirmation.

A `model_fallback` retry does not need a new confirmation: the server retries the same task with the approved objective, `working_dir`, and scope, and only the model changes. The same holds for the server's automatic restart of a run whose log never associated (`restart_of`). Do not ask permission again for either.

A delegated step inherits every invariant (INV-GATE-3). The gate authorizes delegation; it does not authorize delivering less than the approved scope.

## Writing the spec

Write one self-contained step per dispatch. A step that depends on another's output waits until that one reaches `succeeded`. Put INV-QUALITY-1 into the spec: the backend is an external agent working to make the immediate step pass, so write the operating envelope into `constraints` (platforms, harnesses, input classes, callers, and "fix the cause, not the symptom") and write `acceptance` against the contract, not the triggering case. A `succeeded` on a spec with no envelope shows only that the backend satisfied itself. For a palette story, take `objective` and `acceptance` from the approved acceptance criteria, not from the raw Tier-A artifact.

A fallback retry or automatic restart runs the same prompt in the same `working_dir` without resetting files an earlier attempt wrote. When you configure a fallback chain, prefer objectives that converge when re-run over a partly written tree.

## Finishing

Report a delegated step as done only after `dispatch_status` shows `succeeded` and you have reviewed the captured result (INV-VERIFY-1). After a cancel, confirm through `dispatch_status` that the task reached `cancelled`. Each dispatch spends backend quota and runs on its own, so do not fan out speculatively.

dispatch sends no notification when a run finishes, and `dispatch_status` returns a snapshot without blocking, so do not poll it in a tight loop. If you have other useful work, do it and check again later in the same turn. If you are ending the turn, arm a follow-up check with the harness's own wait or scheduling mechanism where one exists. Where none exists, tell the user the task is still running and that they need to ask you to check back. Do not end a turn on an unfinished task with nothing armed and nothing said.

In Claude Code, a backgrounded `Agent` or `Workflow` call notifies the session when it completes; dispatch does not. The mechanism to arm is `ScheduleWakeup` (or a `/loop`), with the interval matched to the task's expected duration.
