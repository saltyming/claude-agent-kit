<!-- slate-agent-kit:common -->
# Dispatch

The `dispatch` MCP server hands an execution step to a coding-agent backend that runs headless, write-capable and asynchronously. How to operate it (submit, status, logs, steer, cancel, the spec fields, reading a quiet log, the server's guards) is in the server's own instructions and tool descriptions. This file covers when a dispatch is worth it and how to hand the step over.

## When to dispatch

Dispatch is for execution, not judgment. It fits isolated mechanical edits, long verify-and-fix loops, large well-scoped repetitive sweeps, and independent plan steps with clear target files and acceptance criteria. Do not dispatch work with an open product question, edits that overlap active local or user changes, work that needs close interactive judgment, or anything that cannot be written as one self-contained spec.

Apply INV-AUTO-1 and the dispatch level (INV-AUTO-2) in `$HOME/.claude/rules/claude-agent-kit--dispatch-prefs.md` (it loads with these rules). At `suggest`, propose the step in one line with its `working_dir`, scope and backend, and wait. At `auto`, submit and say the same in one line. A current-turn instruction ("use dispatch", "do not dispatch") outranks the level. The backend, model, effort and fallback chain come from the prefs unless the user names others.

A `model_fallback` retry and the server's automatic restart of a run whose log never associated (`restart_of`) run the same approved step; neither needs a new proposal.

## Writing the spec

Write one self-contained step per dispatch; a step that depends on another's output waits until that one reaches `succeeded`. The backend works to make the immediate step pass, so write the operating envelope into `constraints` (platforms, harnesses, input classes, callers, and that the cause is fixed, not the symptom) and write `acceptance` against the contract, not the triggering case (INV-QUALITY-1). For a palette deliverable, take `objective` and `acceptance` from its approved `Done when`. A fallback retry or restart runs the same prompt over files an earlier attempt may have written, so prefer objectives that converge when re-run. The dispatched step inherits every invariant (INV-DELEG-2).

## Finishing

Report a step as done only after `dispatch_status` shows `succeeded` and you have reviewed the captured result (INV-VERIFY-1). After a cancel, confirm through `dispatch_status` that the task reached `cancelled`.

dispatch sends no notification when a run finishes, and `dispatch_status` returns a snapshot without blocking, so do not poll it in a tight loop. Do other useful work and check again later in the turn. When ending the turn with a run unfinished, arm a follow-up check where the harness offers one; otherwise tell the user the run is still going and that they need to ask you to check back.

In Claude Code, a backgrounded `Agent` or `Workflow` call notifies the session when it completes; dispatch does not. Under `/loop`, arm `ScheduleWakeup` with an interval matched to the run's expected duration; outside a loop, tell the user the run is still going.
