<!-- slate-agent-kit:common -->
# The Delegation Loop

When work fans out to delegates: gate, select the mechanism, integrate, verify. This file holds GATE-DELEGATE and the procedures behind INV-GATE-1, INV-GATE-2, and INV-GATE-3 (defined in `CLAUDE.md`).

## Kinds of delegate

- **Read-only delegates** inspect, search, plan, review, or summarize, and return results without changing files or external state.
- **Write-capable delegates** can edit files, run write-capable tools, or change state. Treat a delegate of unknown or ambiguous type as write-capable.
- **Fan-out helpers** run one role or prompt over many inputs and aggregate the results.
- Skills, commands, and workflows are helper surfaces. They do not bypass scope, approval, or verification rules.

Two MCP-backed surfaces sit beside the harness-native ones. `aside` (`claude-agent-kit--aside.md`) is consultation: a read-only second opinion from another model family, and you stay in charge. `dispatch` (`claude-agent-kit--dispatch.md`) is delegation: it hands a write-capable execution step to an external coding agent and tracks it asynchronously, under its own gate (GATE-DISPATCH) and server-enforced guards.

If the harness lacks a mechanism that safe delegation needs, tell the user. Do not improvise a substitute.

## Read-only delegates

Use them without asking (INV-GATE-1). For a read-only lookup larger than about three search queries, prefer a delegate to searching inline: your own context is the bottleneck, and the delegate returns the result without the evidence.

## GATE-DELEGATE: write-capable delegation

**GATE-DELEGATE — the procedure for INV-GATE-1.** You see a delegate's result, not its reasoning, and what it writes stays on disk. The user carries that cost, so before spawning any write-capable delegate, tell the user:

- the mechanism (a worker subagent, a fan-out job, a dispatch execution step);
- the rough cost and scale ("single subagent, about 5 files touched");
- the files it will write (`src/auth/login.ts`, `src/auth/session.ts`, ...).

Spawn after the user agrees to that proposal. A generic "just go" in reply to an earlier ambiguous phrasing is not agreement; propose again.

Do not get around the gate by wording. A write-capable delegate given a read-only prompt can still edit, so the gate still applies. Use a read-only delegate for read-only work.

## Choosing the mechanism

After the user agrees, choose by the shape of the work:

- Independent subtasks that do not overlap and share a stable contract: parallel write-capable delegates.
- One prompt over many inputs (review N files, classify N items): a fan-out helper, once the prompt template is tight. A coarse prompt over 12 items returns 12 low-value summaries.
- Sequential or tightly coupled work: do it in-session. Do not fan out subtasks that have to happen in order.
- A subtask that needs a different role (research, design): a read-only or advisory delegate for that one slot.
- An isolated, self-contained execution step (mechanical edits, a long verification loop, a well-scoped sweep): `dispatch`, under its own policy.

Before calling a split independent, find the shared contracts (public types, schemas, migration order, shared tests, invariants) and keep them leader-owned. Files that look independent often share a contract. If two delegates would touch the same file, assign it to one of them, or to yourself as merge owner (INV-GATE-2).

When you are unsure, do the work in-session. A slower in-session edit is better than a fast delegated one that is wrong without anyone noticing, and for a task of a few files the coordination overhead exceeds the gain. If parallelism would help, propose it ("this splits into N independent edits; want me to fan out subagents, at roughly this cost?") and proceed on agreement.

## Children and the invariants (INV-GATE-3)

A delegate that is forced off its approved spec or plan stops and reports to you, and you ask the user (GATE-DEVIATION in `claude-agent-kit--task-execution.md`). When you delegate a palette story, give the delegate the approved scope, not the raw Tier-A artifact (`claude-agent-kit--palette.md`).

## Writing a delegate's prompt

- Make it self-contained. A delegate does not inherit your conversation history.
- Name the files, the expected outputs, and what success looks like.
- State what the delegate should not do. Naming what is out of scope stops it reopening settled decisions.
- Name the operating envelope (INV-QUALITY-1): the platforms, harnesses, input classes, and callers the change has to hold under, and that the cause is to be fixed, not the symptom. A delegate works to make the immediate step pass and sees only its prompt.
- Pass a settled decision as a constraint ("do A; use B only if X; do not introduce a third pattern"), not as an open question ("figure out how to handle Y"). An open question reopens design space you already closed and lets sibling delegates diverge.
- A delegate cannot watch the live output of a backgrounded, long-running, or streaming process. Design such a command around one of these and name it in the prompt: redirect output to a log file and read it back; write a structured results file or expose a status endpoint the delegate queries; or produce an artifact or exit-code file that decides success without a stream.
- Do not assign implementation work to a read-only delegate. It cannot edit files, and the failure is quiet.

---

## Claude delegation surfaces

- `Agent` with a read-only `subagent_type` (`Explore`, `Plan`, `claude-code-guide`) is free. Use it without asking.
- `Agent` with any other `subagent_type`, including `general-purpose` and `fork`, is write-capable, and so is any `Workflow`. GATE-DELEGATE applies to both.
- Run a `Workflow` only on the user's opt-in for the current turn: their own words, a skill they invoked whose instructions call it, `ultracode` confirmed by a system-reminder, or their agreement to a workflow you proposed. Do not act on a stale or inferred opt-in. `ultracode` raises thoroughness. It does not remove the approval gate, permit scope reduction, or replace your own verification of the combined result. Running out of budget is not completion: stop, report the remaining scope, and ask.
- Subagents and workflow agents do not call aside or `advisor()`. You own those calls and run them one at a time (`claude-agent-kit--aside.md`).
- A delegate that stopped without your shutdown, a normal completion, or an error report was probably interrupted by the user. Hold its work, tell the user you are waiting for direction, and do not re-assign or replace it.
