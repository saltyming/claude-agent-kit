<!-- slate-agent-kit:common -->
# Work Leaving the Session

How work leaves the session: subagents, consultation (`claude-agent-kit--aside.md`) and dispatch (`claude-agent-kit--dispatch.md`). Each is used at its level (INV-AUTO-2) after judging its value (INV-AUTO-1); both invariants are in `CLAUDE.md`.

## What each surface is for

- **Subagents** are the harness's own delegates. A read-only one inspects, searches or summarizes; a write-capable one edits. Treat a subagent of unknown kind as write-capable.
- **Consultation** asks another model for an opinion; you stay in charge and nothing is written.
- **Dispatch** hands a self-contained, write-capable execution step to an external coding agent that runs asynchronously.
- Skills, commands and workflows are helper surfaces; they do not bypass scope, approval or verification.

If the harness lacks a mechanism that safe delegation needs, tell the user. Do not improvise a substitute.

## Judging whether to delegate

A delegate saves your context but costs the user models, quota and time, and you see its result, not its reasoning. Delegation helps when the work splits into independent parts with a stable shared contract, or when one bounded lookup would flood your context. Do the work in-session when it is sequential or tightly coupled, when it is a few files, when the user is discussing with you and waiting for your own answer, or when a document assigns the work to the leader. A slower in-session edit is better than a fast delegated one that is wrong without anyone noticing.

At `auto`, state in one line before starting: how many delegates, which model, what each does, and which files each writes. At `suggest`, say the same and wait. The model is the one the prefs name unless the user names another for the turn; a read-only delegate does not need the session's top model.

## Splitting the work

- Find the shared contracts first (public types, schemas, migration order, shared tests, invariants) and write them yourself before delegating. Files that look independent often share a contract.
- Give each file one writer; you are the merge owner of any file several delegates need (INV-DELEG-1).
- One prompt over many inputs needs a tight prompt template; a coarse prompt over twelve items returns twelve low-value summaries.
- A subtask that needs a different role (research, design) gets a read-only delegate for that one slot.

## Delegates and the invariants

A delegate that is forced off its approved spec stops and reports to you, and you ask the user (INV-DELEG-2, GATE-DEVIATION in `claude-agent-kit--task-execution.md`). When you delegate a palette deliverable, give the delegate the approved scope and its `Done when`, not the raw palette documents.

A delegate may call the harness's native advisor. It does not consult aside or start dispatch unless the user approved that for this delegation, and then its prompt says so.

## Writing a delegate's prompt

- Make it self-contained: a delegate does not inherit your conversation.
- Name the files it owns, the expected output, and what success looks like.
- State what it must not do and which decisions are settled; pass a settled decision as a constraint, not as an open question, or sibling delegates diverge.
- Name the operating envelope (INV-QUALITY-1): platforms, harnesses, input classes and callers, and that the cause is fixed, not the symptom.
- A delegate cannot watch a long-running or streaming process; have it write a log, a results file or an exit-code file that decides success.
- Do not give implementation work to a read-only delegate: it cannot edit, and the failure is quiet.

---

## Claude delegation surfaces

- `Agent` with `subagent_type` `Explore`, `Plan` or `claude-code-guide` is read-only; any other type, including `general-purpose` and `fork`, is write-capable. An `Agent` call without `model` runs on the default the prefs set (`CLAUDE_CODE_SUBAGENT_MODEL`), else on the session's model; pass `model` when the prefs default does not fit the job.
- `Workflow` orchestrates many agents. Run it only on the user's opt-in for the current turn: their own words, a skill they invoked whose instructions call it, `ultracode` confirmed by a system-reminder, or their agreement to a workflow you proposed. `ultracode` raises thoroughness; it does not remove approval, permit scope reduction, or replace your own verification of the combined result. Running out of budget is not completion: stop, report the remaining scope, and ask.
- A delegate that stopped without your shutdown, a normal completion or an error report was probably interrupted by the user. Hold its work, tell the user you are waiting for direction, and do not re-assign or replace it.
