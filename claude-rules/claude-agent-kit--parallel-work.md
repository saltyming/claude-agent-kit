<!-- claude-agent-kit -->
# Parallel Work

Three mechanisms parallelize work — two via the `Agent` tool, one via the separate **`Workflow`** tool:

- **Subagents** — `Agent` fire-and-forget: runs once, returns a result.
- **Teammates** — `Agent` **named** + run **in the background** (`run_in_background: true`): a persistent peer that keeps running, shares the task list, and exchanges messages with other teammates. Subagents and teammates both spawn through the same `Agent` tool into a **single implicit team** (no `TeamCreate`; `team_name` is deprecated/ignored) — the difference is lifecycle, not a separate system.
- **`Workflow`** — a *separate tool* that runs a deterministic script orchestrating many subagents (fan-out / pipeline) in the background; for large, breadth-first work. It is **not** a `subagent_type`, and is unrelated to this manual's "Three-Phase Workflow" / "Leader Workflow" headings. See *Workflow* below.

A **fourth** delegation surface sits alongside these but is governed by its own file: **`dispatch`** (`claude-agent-kit--dispatch.md`). The three above spawn *Claude* agents in-process; `dispatch` is **hierarchical** delegation that hands an execution step to an **external** coding agent (codex) running write-capable in a target directory, asynchronously (`dispatch_submit` → poll / `dispatch_wait` → `dispatch_cancel` / `dispatch_steer`). Reach for it to *entrust execution to a different agent process*, not to parallelize Claude subagents. It shares this file's **write-gate** (confirm before the first dispatch — see *Delegation* below) and the **scope-integrity** rules a delegated child inherits, but its own mechanics — the approval gate, the server guards, the `dispatch_*` tool flow — live in `claude-agent-kit--dispatch.md`. dispatch is the *execution* complement to `aside`'s read-only *consultation* (`claude-agent-kit--aside.md`).

## Delegation: when and how to engage it

**Read-only *subagents* are free — use them proactively.** An `Explore`, `Plan`, or `claude-code-guide` subagent reads widely and returns only a summary — it *reduces* the leader's context cost. Reach for these without asking; they are **not** gated. This exemption is for read-only *subagents* only: a `Workflow` — even a read-only research one — fans out many agents and is **cost-gated regardless of whether it writes** (see *Workflow* below). So "read-only ⇒ free" holds for subagents, **not** for workflows; everything below applies to *write-capable* delegation **and to any `Workflow`**.

For write-capable delegation you are a collaborator, not a silent executor. When a task's shape fits, **proactively surface and propose** the delegate — name the mechanism, its rough cost/scale, and which files it would write — and proceed **on the user's agreement**. Don't wait to be asked; and don't spawn a write-capable delegate without that agreement.

**Keep two things distinct:**

1. **The gate — whether to delegate writes at all.** Any write-capable delegate (a `general-purpose` subagent, an Agent-Team teammate, or a `Workflow` that edits files) spawns only after the user agrees to a concrete proposal: *mechanism + rough cost/scale + the files it will write*. The gate exists because delegated writes are durable on disk, and you see only the delegate's **summary** — not its reasoning or intermediate tool output — so a misread becomes a committed mistake that is costly to take back. The user owns the decision to incur that.

   The gate is on **capability, not the prompt you plan to send**: if the spawned `subagent_type` *could* edit files, it is gated, even if you intend "just read things." Use `Explore` for a genuine read-only lookup; don't reach for `general-purpose` with a read-only prompt as a workaround. Default for an unknown/ambiguous `subagent_type`: treat as write-capable. (aside / `advisor()` are consultations, not file-mutating delegates — out of scope for this gate; see `claude-agent-kit--aside.md`.)

2. **The selection — which mechanism — decided only after the gate, by dependency structure:**
   - **Independent, non-overlapping, contract-stable subtasks** → fire-and-forget **subagents** (a few per turn).
   - **Coordinated streams that must talk to each other** → an **Agent Team** (shared `depends_on` graph + teammate messaging + mid-turn steering give the leader live coordination and steering that blind fan-out can't).
   - **Large, breadth-first, mechanical sweeps** (codebase-wide hunt, N-file migration, cross-checked research) → a **`Workflow`** (see *Workflow* below).

   Before calling any split "independent," identify the shared contracts first — public types/APIs, schemas, migration ordering, shared tests, invariants — and keep those leader-owned. Independent-looking files often share a contract.

**One writer per final target file.** No two delegates edit the same file; worktree isolation prevents disk clobber but not divergent edits, so a shared file needs one writer, or one explicit merge owner (the leader).

**Delegated children inherit the scope rules.** A teammate or `Workflow` child that hits a forced spec/plan deviation stops and reports to the leader, who asks the user — a child never shrinks scope, reinterprets a budget, or substitutes a cleaner design on its own (same rule as `claude-agent-kit--task-execution.md`).

**palette note.** When delegating a palette story, hand the delegate the *approved, tracked scope* (as a dispatch spec or team task), never the raw `story-*.rst`. A story's acceptance criteria pass the approval gate first, then become the delegate's brief — a child receives Tier-B scope, not a Tier-A artifact. See `claude-agent-kit--palette.md`.

**Effort/model control differs by surface.** The interactive `Agent` tool exposes `model` but no reasoning-effort knob — a delegated `Agent` runs at the CLI's default reasoning level, which the leader can't raise. The `Workflow` `agent()` *does* expose `effort` (and `model`) — set them explicitly (they don't reliably inherit). So a `Workflow` can be the more controllable surface for large fan-out once opted in. This `Agent`-tool gap would only be revisited if the `Agent` tool itself gained effort control; `Workflow` having it does not change the `Agent`-tool calculus.

**When unsure, do the work in-session and propose.** A slower in-session edit beats a fast-but-silently-wrong delegated one. If parallelism would genuinely help, surface it ("this splits into N independent edits — want me to fan out subagents, ~rough cost?") and proceed on agreement.

## Choosing Between Subagents and Teammates

| | Subagents (`Agent`, fire-and-forget) | Teammates (`Agent` with `name` + `run_in_background`, single implicit team) |
|---|---|---|
| Communication | Results returned to parent only | Teammates message each other directly |
| Coordination | Parent manages everything | Shared task list with self-claiming |
| Context | Own window; result summarized to parent | Own window; loads CLAUDE.md, MCP, skills |
| Task system | None (prompt = task) | `workslate_task_*` with dependencies + SQLite WAL concurrency |
| Best for | **Independent** subtasks (no coordination, no mid-task steering) | **Dependent / coordinated** work (shared `depends_on`, cross-talk, mid-turn steering) |
| Token cost | Lower | Higher (each teammate is a full Claude instance) |

Selection follows the dependency-structure decision rule already stated above in *Delegation* → "Keep two things distinct." All three are write-gated identically — surface/propose → agree; the choice is which fits the work's shape, not whether you're allowed.

## Spawn mechanism (read this before picking a parallel tool)

The `Agent` tool is the **spawn mechanism for both** subagents and teammates. The discriminator is **`run_in_background` + `name`**, not `team_name` (which is deprecated and ignored — there is one implicit team per session):

| Invocation | What you get |
|---|---|
| `Agent(subagent_type=..., prompt=..., ...)` | A **subagent**. Fire-and-forget, own context window, no peer messaging. The result returns to the parent when it finishes. |
| `Agent(name=..., subagent_type=..., model=..., run_in_background=true, prompt=...)` | A **teammate** in the implicit team. Named (peers address it by name via `SendMessage`) and backgrounded (it keeps running across the leader's turns, can be steered mid-task, and is resumable via `SendMessage` to its name/ID). Shares the task list (`workslate_task_*`), runs until `shutdown_request`. |

Key facts that the rest of this document builds on:

- **There is no team container to create.** The implicit team always exists; you populate it by calling `Agent(name=..., run_in_background=true, ...)` once per teammate. Add more teammates mid-run with additional such calls — there is no `TeamCreate` / `TeamDelete`.
- **`run_in_background=true` is what makes a teammate steerable.** Without it, even a named `Agent` call runs to completion before returning, so the leader cannot message it mid-task. Backgrounding is required for the mid-turn doorbell steering described below.
- **`subagent_type` controls what the spawned agent can do.** Read-only types (e.g., `Explore`, `Plan`) cannot edit or write files — never assign them implementation work, whether as a subagent or a teammate. Use a full-capability type (e.g., `general-purpose`) for teammates that must modify code.
- **`model` controls cost.** Default teammates to `model="sonnet"`; escalate to `opus` only where documented below.

Whenever this document says "spawn a teammate," read that as "call `Agent` with `name` set and `run_in_background=true`." Whenever it says "spawn a subagent," read that as "call `Agent` fire-and-forget (no `name` / not backgrounded)." The rest of the rules (role-only creation prompts, self-claiming, completion report format, etc.) are behavioral and apply on top of the same underlying tool call.

## Subagents

Lightweight workers spawned via the `Agent` tool fire-and-forget (no `name`, not backgrounded). Execute a task and return a result — no inter-agent communication, no mid-task steering.

**Prompt rules:**
- Prompts must be **self-contained** — include all necessary context inline
- Subagents load CLAUDE.md but do not inherit the parent's conversation history
- Specify exact file paths and expected outputs
- State what the subagent should NOT do
- **Frame settled decisions as constrained choices, not open questions.** When a subtask carries a decision you've already made, hand the agent the framed choice ("do A; use B only if X; don't introduce a third pattern") rather than an open prompt ("figure out how to handle Y"). An open prompt invites the agent to re-open settled design space — burning tokens and risking divergence from what sibling lanes assumed. Pre-frame the decision; cap the leeway.
- **A subagent can't watch the live output of a backgrounded, long-running, or streaming process.** Foreground tool output returns in the `tool_result` normally — but a subagent has no live terminal to watch a process it launched in the background, one that streams over time, or output that another agent must consume asynchronously. A command fired assuming the agent will "watch it run" is flying blind there. Design such commands around one of three escapes, and name the chosen one in the subagent's prompt:
  1. **Write-then-read** — redirect to a log file and read it back (`cmd > run.log 2>&1`, then read `run.log`). The file is the feedback channel.
  2. **Observability sink** — a structured channel the agent queries: a results file in a known schema, a status endpoint, a test report it parses.
  3. **Output-free success check** — make success/failure decidable without a stream: assert on a produced artifact, capture an exit code to a file, or (in a `Workflow`) use a `schema:` return.

  (The subagent's view of its *own* backgrounded process — distinct from the leader seeing only a delegate's summary, not its intermediate tool output, noted under *Delegation* above.)

**When to use (read-only subagent types — proactive is fine):**
- `subagent_type="Explore"` for broad codebase research that would take more than ~3 Grep/Glob queries.
- `subagent_type="Plan"` for read-only design sketches / architecture exploration.
- Other advisory-only types (`claude-code-guide`, etc.) for their documented scope.

**When to use (write-capable subagent types — surface/propose → agree, per *Delegation* above):**
- `subagent_type="general-purpose"` for parallel implementation, build/test verification that writes files, or any delegated work that can edit or create files — when the subtasks are genuinely **independent**.
- Rule: surface and propose the fan-out (mechanism + rough cost + the files it will write); spawn on the user's agreement. Don't spawn `general-purpose` subagents silently.

**Naming:** `agent-<domain>` (e.g., `agent-vfs`, `agent-core`)

## Agent Teams

A coordination system for multiple Claude Code instances that work together via shared task lists and direct messaging — the right tool for **dependent, coordinated** work. Requires `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` in settings/env; see *Known Limitations* below for the operational constraints to plan around.

### Agent Teams cost more — match them to coordinated work

Each teammate is a full Claude Code instance. On spawn, each teammate independently loads CLAUDE.md, every MCP server, and every skill. Once running, every completion report, idle notification, and status update flows through the leader's context. A 5-teammate team spends roughly 3–5× the tokens of the same work done in a single session.

**Scale criteria — use these, not "the work feels parallel":**

| Scope | Default — do in-session, or surface/propose | If the user agrees to the proposal |
|---|---|---|
| < 5 files to modify | Single session. No team, no subagents. | Single session even if asked — the overhead isn't worth it. Explain why. |
| 5–10 files, cross-cutting concerns | Single session. Optionally use `Explore` subagents for read-only research. | Leader session + 1–2 `general-purpose` subagents for isolated edits; leader integrates. |
| 10+ files with clean, non-overlapping file scopes | Single session (propose parallelism to the user if you think it would help). | Agent Team is justified. |
| 10+ files but scopes overlap / shared types dominate | Single session. | Still single session — coordination overhead exceeds the parallelism win. Tell the user. |

**Scale informs *what you propose*, not *whether* to spawn unprompted.** The criteria describe when parallelism is viable; the spawn still happens only on the user's agreement to a proposal (per *Delegation* above). A 30-file refactor handled in-session is a fine default; if a team would genuinely help, surface it ("this has N coordinated streams — want me to spawn a team, ~rough cost?") and proceed on agreement.

### Model choice for teammates

Teammates are spawned by calling `Agent(name=..., subagent_type=..., model=..., run_in_background=true, prompt=...)` once per teammate (see **Spawn mechanism** above — there is no `TeamCreate`). The `model` parameter (`sonnet` | `opus` | `haiku`) is the single biggest lever on team cost after team size — pick deliberately. The `subagent_type` parameter is equally load-bearing: pick wrong and the teammate cannot perform its role.

- **Default teammates to Sonnet** — `Agent(name=..., model="sonnet", run_in_background=true, ...)`. Teammate work is well-scoped: claim an unblocked task, edit files inside an assigned scope, produce a completion report. Sonnet handles this reliably at a fraction of Opus token cost, and the leader (on Opus) is where cross-teammate reasoning happens anyway.
- **Leader stays on Opus** — inherited from the current session, no override needed. The leader designs the task graph, reconciles conflicting assumptions between teammates, and owns integration/verification. Weakening the leader to save tokens usually costs more in rework.
- **Escalate a specific teammate to Opus only for genuine reasoning load** — e.g., a `verifier-review` / semantic reviewer that must catch subtle contract mismatches across modules, or an `arch-designer` making cross-cutting design calls. Note the exception in the creation prompt so future readers know why that teammate is not on the default.
- **Model choice does not license scope shrinkage.** Sonnet teammates are still bound by the "task scope is non-negotiable" rule — if a Sonnet teammate cannot complete the task as specified, they report to the leader rather than silently trimming it.
- **Pick `subagent_type` to match the role.** Implementation teammates need a full-capability agent type (e.g., `general-purpose`) — read-only types like `Explore` or `Plan` literally cannot edit or write files, so handing them an implementation task produces silent failure. A `verifier-build` teammate that only runs build/test can use `general-purpose` (it needs Bash). A pure-research teammate with no file edits can use `Explore`. When in doubt, default to `general-purpose` — it is the only built-in type that can both read and write.

**How teammates work (system-level guarantees):**
- Teammates load **CLAUDE.md, MCP servers, and skills** automatically (same as any Claude Code session)
- Teammates do NOT inherit the leader's conversation history
- Messages between teammates are delivered **automatically** (no polling) — this is built-in `SendMessage`, which we use

**Two task systems — keep them distinct:**
- **Claude Code's built-in Agent Team task list** (`TaskCreate` / `TaskList` / `TaskUpdate`, with `blockedBy`) is a *separate* system with native file-locking self-claim, automatic dependency unblock, and system-delivered assignment. **This project does NOT use it for team coordination.**
- **`workslate_task_*`** (`ws:` / `team:` namespaces, `depends_on`) is our coordination + tracking system — and the one the **doorbell footer surfaces on every tool call**. Here self-claim is *behavioral* (see Task Claiming Policy below — not a system file-lock), dependency unblock is handled by workslate, and a teammate sees a new assignment via the doorbell, not via built-in auto-delivery.

### When to Use

**Use Agent Teams when ALL of these hold:**
- **You surfaced/proposed an Agent Team and the user agreed** (or the user asked for one directly). This is the write-gate; the bullets below describe *viability* — when a team actually fits — not permission.
- 3+ independent work streams can run in parallel.
- Teammates need to share findings or challenge each other.
- Work requires discussion and collaboration (competing hypotheses, cross-layer changes).

**Do NOT use when:**
- You have not surfaced it and gotten agreement. Propose it first ("this has 4 coordinated work streams — want me to spawn an Agent Team, ~rough cost?") and proceed on agreement. Do not spawn a team just because the scale criteria happen to match.
- Work is sequential (each step depends on the previous).
- Only 1-2 files need modification.
- Workers do not need to communicate (use subagents instead — which are themselves write-gated the same way: surface/propose → agree).

### Team Composition

**Naming:**

| Element | Convention | Example |
|---------|-----------|---------|
| Team name | kebab-case, describes objective | `auth-refactor` |
| Leader (you) | the reserved role `team-lead` — register under it on startup | `team-lead` |
| Teammates | descriptive role name | `security-reviewer`, `arch-designer` |

**Team size:** 3-5 teammates for most workflows. 5-6 tasks per teammate keeps everyone productive.

**Task granularity:** Each task should produce a clear, self-contained deliverable (a module, a test file, a handler). Too small (single function) = coordination overhead exceeds benefit. Too large (entire subsystem) = self-claiming loses meaning and risk of wasted work increases. A good task takes a teammate roughly 5-15 minutes of focused work. Task granularity is a core leader skill — poor granularity undermines the entire task graph.

### Leader Workflow

The leader's role is **task graph architect + build/integration owner**, not task dispatcher.

The leader uses **workslate tasks with namespaces** for unified tracking:
- `ws:` namespace — leader's personal phases (understand, plan, integrate, verify)
- `team:` namespace — team work assignments with owner and dependencies

Both namespaces appear in the footer. The leader sees `ws:[2/4] team:[8/12]` at a glance.

```
1. workslate_task_init(<name>, session_id=<S>)   → Create a named session for this team effort (session_id from the SessionStart hint)
2. workslate_register(role="team-lead", session_id=<S>, agent_id="")
                                                 → Register yourself as team-lead (agent_id="" — the main session) so teammate messages to "team-lead" reach you and the inbox doorbell nudges you; then workslate_inbox_read(role="team-lead")
3. Agent(name=..., subagent_type=..., model="sonnet", run_in_background=true, prompt=<role-only>)
                                                 → Spawn each teammate into the implicit team (no TeamCreate). Call Agent once per teammate. Teammates explore their scope while waiting — see Creation Prompt below.
4. workslate_task_create(namespace="team")       → Design task graph with depends_on and owner
5. Teammates work                                → Self-claim eligible tasks via workslate_task_update(owner=self)
6. Monitor                                       → Footer shows team progress; intervene only when stuck
7. Build & verify                                → After all teammates complete
8. Fix integration                               → Missing imports, visibility, mod declarations
9. Shutdown                                      → shutdown_request to each teammate (no TeamDelete — the implicit team needs no teardown)
```

**Creation prompt rules:**

Creation prompts must describe **role and file scope only** — not specific tasks. This prevents teammates from starting implementation before tasks exist.

```
# Good — role + scope, no actionable work
"You are the auth module teammate. Your scope is src/auth/.
Read and understand the code in your scope while waiting for task assignments."

# Bad — teammate will start implementing immediately
"Refactor src/auth/ into 3 modules: types.rs, middleware.rs, handlers.rs"
```

**Leader responsibilities:**
1. Create team with **role-only creation prompts** (no specific tasks in the prompt)
2. **Design the task graph** — proper scope, `depends_on` dependencies, leader-reserved marking
3. Mark shared types / integration / cross-scope tasks as **leader-reserved** (assign owner to leader via `workslate_task_update`)
4. **Run build & tests** — teammates may lack Bash permissions
5. Fix integration issues after all teammates complete
6. Shutdown all teammates (`shutdown_request`) when the work is done
7. **Route discovered gotchas** — when a completion report carries a `GOTCHA`, place it (do not build a parallel store): durable / cross-session → persist to native memory (a memory file + `MEMORY.md` pointer); run-local → inject it into not-yet-spawned sibling subagents' prompts, or message running teammates via `workslate_msg_send`. The channels already exist; the line only makes the hand-off explicit.

**Leader checklist:**
- [ ] Registered yourself as `team-lead` (`workslate_register(role="team-lead", session_id=<S>, agent_id="")`) so teammate messages reach you and the inbox doorbell fires
- [ ] Teammates spawned with `model="sonnet"` unless a specific role justifies Opus (document the exception in the creation prompt)
- [ ] Teammates spawned with a `subagent_type` that matches the role — implementation teammates MUST use a write-capable type (e.g., `general-purpose`); never `Explore` or `Plan` for implementation work
- [ ] Creation prompts contain role/scope only (no implementation instructions)
- [ ] Task graph designed with proper `depends_on` dependencies
- [ ] Shared types / integration / public interface tasks reserved to leader (owner = leader)
- [ ] Each teammate's file scope does not overlap
- [ ] Build executed after teammates report completion
- [ ] All teammates shut down (`shutdown_request`) when done

### Leader Intervention

The leader must actively monitor, not just wait for completion reports. Intervene when:

- **Inconsistent assumptions** — multiple teammates' reports reveal conflicting interpretations of shared types, APIs, or contracts. Fix: pause affected teammates, clarify the contract, then resume.
- **Silent stall** — a task is unblocked and claimed but no progress or report arrives. The teammate may be stuck without recognizing it as a blocker. Fix: message the teammate to check status.
- **Downstream failure** — a task completes but dependent tasks fail to proceed or produce unexpected results. The upstream output may be subtly wrong. Fix: review the completed task's output before letting dependents continue.
- **Scope drift** — a completion report shows files modified outside the teammate's assigned scope. Fix: revert or reassign, reinforce scope boundaries.
- **Duplicated work** — two teammates produce overlapping implementations (e.g., both define the same helper type). Fix: choose one, remove the other, update task graph.
- **User-initiated teammate interrupt — do NOT intervene.** When a teammate stops in a way that did **not** come from (a) a `shutdown_request` you sent, (b) a normal task completion, or (c) an explicit error/blocker report from the teammate, assume the stop was a **user action on that teammate** (the user interrupted the teammate's turn directly — e.g., to inspect it, redirect it, or pause it). Do **not** re-assign the task to another teammate, do **not** pivot to an alternate approach, do **not** spawn a replacement, and do **not** try to infer what the teammate "would have done." Wait for the user's next instruction; surface a brief "waiting for user direction — teammate X was interrupted, task Y held" note so the state is visible. If you genuinely cannot tell a user interrupt apart from a real crash/timeout, ask the user before any recovery action — a silent wait is cheaper than a wrong pivot.

The leader does NOT need to review every completion report in detail. Skim reports for red flags (unexpected files, cross-module references outside scope, ambiguous contract descriptions) and investigate only those.

### Teammate Behavior

> **Teammates read this section directly** — CLAUDE.md is loaded by all teammates.

**When you are a teammate in the implicit team, follow this work loop:**

1. **On creation:** Read and explore code within your assigned scope. **Do NOT start implementing anything.** Wait until tasks appear in the task list.
2. **Self-claim** an eligible task (see Task Claiming Policy below).
3. **Work** on that task only. Stay within your assigned file scope.
4. **On task completion:** Send a **completion report** to the leader (`recipient="team-lead"`) using the format below, then self-claim the next eligible task. If no eligible task exists, drain your inbox before idling (step 7).
5. **On blocker:** Report to the leader (`recipient="team-lead"`) immediately and wait.
6. **On `shutdown_request`:** Finish current work and shut down gracefully.
7. **Before idling / entering a wait state:** as your **final action**, call `workslate_inbox_read(role=<your name>)`. The per-tool-call inbox doorbell only fires while you are running tools — once you idle you make no tool calls, so a leader `msg_send` that landed during your last working turn would sit unseen. Drain it now; if a message is actionable, handle it instead of idling. This is a drain-*before*-idle, not a poll while suspended — once your turn ends you are suspended and only the leader's built-in `SendMessage` can wake you (see *Mid-Turn Steering* below).

**Rules:**
- Do not run build/test directly — request the leader to do it
- Do not touch files outside your assigned scope
- If task ownership is ambiguous, ask the leader instead of claiming
- **Do not reduce task scope.** Implement the entire task as specified. If you believe the scope is too large or contains an error, report to the leader BEFORE starting — do not silently skip parts. "Simplify" or "minimal implementation" is not an acceptable reason to cut scope.
- **Notify affected teammates directly.** When your output (types, APIs, file formats, constants) is used by another teammate's task, message that teammate with what you produced. Do not assume they will discover it on their own.

### Completion report format (HARD RULE)

Every completion report must be plain text, under ~500 tokens, and follow this exact structure:

```
TASK: <id> — DONE

CHANGED:
- <file:line-range>: <1-line summary of what changed>
- <file:line-range>: <1-line summary of what changed>

VERIFICATION:
- <grep check, invariant confirmed, types compile, etc — concrete evidence>

DEFERRED (optional, omit if none):
- <thing intentionally not touched and why>

GOTCHA (optional — a distilled one-line trap, not process narration; include ONLY if it should change a sibling prompt, a future decomposition, or native memory):
- <trap → how to avoid, 1 line>

NEXT: <ready for task X / shutdown / blocked on Y>
```

**Rules for the report:**
- Do not narrate your process. "I started by reading X, then I considered Y, then I chose Z" is noise. The final state is what matters.
- Do not describe each hunk. The leader has the diff.
- Do not paste code. If the leader needs details, they will read the file.
- Prefer file:line references over prose descriptions of location.
- `VERIFICATION` must contain **concrete evidence**, not assertions. "grep 'fn old_name' returns 0 matches" is evidence. "types are correct" is not.

Long, narrative completion reports waste the leader's context and delay the next task assignment. A disciplined report is a sign of a disciplined teammate.

### Task Claiming Policy

> **This section is enforced via CLAUDE.md behavioral rules, not by the system.** The system allows any teammate to claim any unblocked task. These rules constrain that.

**Teammates may self-claim a task when ALL of these conditions are met:**
1. The task is **unblocked** (all `depends_on` dependencies completed)
2. The task is **unassigned** (no owner set)
3. The task is **within the teammate's assigned file scope**
4. The task does **NOT** modify shared files, shared types, or public interfaces

**Tasks the leader must handle directly (leader-reserved):**
- Shared types / constants / public interface definitions
- Integration and final wiring tasks
- Cross-scope tasks that touch multiple teammates' files
- Any task where ownership is ambiguous

Leaders mark these as reserved by assigning owner to themselves via `workslate_task_update`. Teammates must not claim tasks that already have an owner.

**When multiple eligible tasks are available, prioritize in this order:**
1. Tasks on the **critical path** — tasks that other tasks list in their `depends_on` (unblocking others has the highest throughput impact)
2. Tasks with the **most dependents** — prefer unblocking 3 teammates over unblocking 1
3. Tasks **relevant to current work context** — minimize context switching as a tiebreaker

### File Conflict Prevention

**Cardinal rule: no two teammates modify the same file** (full rationale in *Delegation* → "One writer per final target file" above).

- Each teammate's file scope is defined in their creation prompt
- Shared dependencies (types, constants) get their own task; other tasks depend on it via `depends_on`
- If two teammates must touch the same file, assign it to exactly one

### Mid-Turn Steering & Team Messaging (workslate)

The built-in `SendMessage` delivers to a teammate only at its **next turn boundary** — a teammate part-way through a long multi-tool-call turn does not see the message until it finishes, by which point it may have completed work in the wrong direction. workslate adds a messaging layer with a **per-tool-call doorbell** to close this gap.

**Tools:**
- `workslate_register(role, session_id, agent_id)` — map this Claude session to your role name (e.g. `"backend-dev"`). Pass `session_id` **and** `agent_id` = the values from the workslate `SubagentStart` hint (`[workslate] agent_id=… session_id=…`). A subagent shares its parent's `session_id`, so `agent_id` is what tells the doorbell which agent you are; the MCP server's own env id does NOT match the hook's, so both must come from the hint. Call once on startup.
- `workslate_msg_send(recipient, subject, body, urgent?, session_id?, agent_id?)` — send a message to a role's inbox in the active task session. `subject` is the one-liner shown in the doorbell; `body` is read on demand; `urgent=true` flags it 🚨. Pass `session_id`/`agent_id` (the same hint values you give `register`/`task_init`) so the sender is attributed to *your* role via the composite `(session_id, agent_id)` identity — without them, sender falls back to a process-shared `active_role` cache that is wrong when a leader and teammates share one MCP server process.
- `workslate_inbox_read(role)` — return unread messages addressed to your role and mark them read (atomic — concurrent reads never double-deliver).

**How delivery works.** `make install` registers four hooks: a wildcard `PreToolUse` **inbox** doorbell and a wildcard `PostToolUse` **task** doorbell that run on every tool call in every session — including teammates, since a teammate inherits the lead's hook config — a `SessionStart` hook that hands the **main** session its `session_id`, and a `SubagentStart` hook that hands each **subagent** its `agent_id` and `session_id` (subagents do NOT fire `SessionStart`, so this is their only identity hint; see startup sequence). The **inbox doorbell** injects a one-line nudge (`📨 N unread … Latest: "…"`) until you call `workslate_inbox_read`; the **task doorbell** injects the task-status footer (on `PostToolUse`, so it reflects the just-run tool's own effect — e.g. a `workslate_task_done` shows done on that same call, not one later — while the inbox nudge stays on `PreToolUse` so it never misses a call, including ones that error or are denied). The doorbell is only a *notification* — the model still chooses to pull the body via `workslate_inbox_read`. For hard steering, mark the message `urgent`.

**Teammate startup sequence (required):** as a subagent you receive a `SubagentStart` hint `[workslate] agent_id=<A> session_id=<S>` (NOT a `SessionStart` hint — subagents do not fire that, and they share the parent's `session_id`). Pass **both** to: `workslate_task_init(<same session name the leader used>, session_id=<S>, agent_id=<A>)` → `workslate_register(role=<your name>, session_id=<S>, agent_id=<A>)` → `workslate_inbox_read(role=<your name>)`. `agent_id` is required because the parent and every subagent share one `session_id`; the composite `(session_id, agent_id)` is what separates your inbox from the leader's. The leader must propagate the task-session name to teammates in their creation prompt.

**Leader startup sequence (required):** the leader is the **main** session, so its `SessionStart` hint carries only `session_id=<S>` (no `agent_id`). Register and read your inbox once on startup: `workslate_task_init(<name>, session_id=<S>)` → `workslate_register(role="team-lead", session_id=<S>, agent_id="")` → `workslate_inbox_read(role="team-lead")`. Pass `agent_id=""` (empty) — that *is* the main session's identity (`(session_id, "")`, shown as `<main>`). Without this registration the inbox doorbell cannot resolve your row and will never nudge you, and your own outgoing `msg_send` has no `(session_id, "")` row to attribute the sender to.

**HARD RULE — every `msg_send` passes BOTH `session_id` and `agent_id`.** The leader and all teammates **share one `session_id`** (subagents inherit the parent's); `agent_id` is the only discriminator, and the leader owns the `(session_id, "")` slot as `team-lead`. **`msg_send` rejects** a call that has a session in effect (passed or env-resolved) but **omits `agent_id`** — it errors and tells you to pass it. Without that guard the omitted id would default to `""`, collide with the leader's `(session_id, "")` row, and silently mis-attribute the message as `team-lead` (and a wholly missing session would fall back to the process-shared `active_role` cache, also usually `team-lead`). So pass your own `session_id` **and** `agent_id` (the `SubagentStart` hint values) on every `msg_send`, exactly as for `register` / `task_init`. An explicit `sender` argument bypasses the guard — the caller takes responsibility; the leader sends with its own `agent_id=""`. **`workslate_register` enforces the same guard** — an omitted `agent_id` there is rejected too, because its `ON CONFLICT` *overwrites* `role`, so a teammate that forgets it would clobber the leader's `team-lead` registration (worse than one mis-attributed message). `task_init` writes the same row but **preserves** `role` on conflict and is also called by solo/main sessions, so it is intentionally NOT guarded — an omitted `agent_id` there is harmless.

**Addressing is by role, not session.** Messages are addressed to the durable role name, so a respawned teammate of the same role still receives prior messages. Assumes **one teammate per role** within a task session — if two live sessions share a role, one `inbox_read` consumes both their messages.

**workslate messaging vs. `SendMessage`:** use `SendMessage` for fire-and-forget peer notes that can wait for a turn boundary; use workslate `msg_send` + the doorbell when you need the recipient nudged mid-turn, durable delivery across respawns, or leader→teammate steering.

**Steering a teammate that may already be idle — send BOTH channels.** The workslate doorbell only fires on the teammate's *own* tool calls; a teammate that has gone idle (its turn ended → it is suspended) makes none, so a `msg_send` alone sits unread until something else wakes it. To re-engage a suspended teammate the leader sends **both**: (a) `workslate_msg_send(recipient="<teammate-role>", subject, body, urgent=true, session_id=<S>, agent_id="")` — the durable, content-rich steering the teammate drains on resume; **and** (b) a built-in `SendMessage` to that teammate whose body tells it to run `workslate_inbox_read(role=<its own role>)`. The `SendMessage` is what actually wakes it at its next turn boundary; the workslate message is what it reads once awake. (Teammate-idle itself surfaces to the leader as a built-in idle notification, so the leader knows when to do this.) For a still-running teammate the doorbell + `urgent` flag suffice — the dual-channel is specifically for the already-idle case.

### Communication

| Situation | Method | Notes |
|-----------|--------|-------|
| Task completion | `message` to `team-lead` | Include completion report |
| Sharing findings | `message` to specific teammate | Direct teammate-to-teammate |
| Blocker | `message` to `team-lead` | Immediate |
| Critical issue | `broadcast` | Rarely — cost scales with team size |
| Shutdown | leader sends `shutdown_request` | After confirming completion |
| Verification fail | `message` to implementer + `team-lead` | Verifier reports bug to implementer directly, notifies the leader that feedback was sent |
| Verification pass | `message` to `team-lead` | Verifier confirms build/test clean |

**Teammate-to-teammate triggers (when you MUST message another teammate directly):**
- Your output defines types, constants, or APIs that another teammate's task consumes → message them with the signatures/paths
- You discover a bug or assumption conflict in another teammate's completed work → message them directly, then inform the leader
- Your task's deliverable changed shape from what was originally planned (e.g., different function name, different file location) → message all teammates whose tasks depend on yours

**Rules:**
- Refer to teammates by name (never UUID)
- Plain text messages only
- Do NOT use SendMessage to coordinate task dependencies — the task system handles this automatically

### Common Patterns

**Pattern 1: Parallel Module Decomposition**
```
Leader creates tasks: types (T1), core (T2 depends_on T1), io (T3 depends_on T1), misc (T4)
├── teammate-types  → Claims T1, extracts shared types
├── teammate-core   → T1 completes → auto-unblocks T2 → claims T2
├── teammate-io     → T1 completes → auto-unblocks T3 → claims T3
└── teammate-misc   → Claims T4 immediately (no dependency)
```

**Pattern 2: Competing Hypotheses**
```
Leader creates investigation tasks, one per hypothesis
├── teammate-a → Investigates theory A
├── teammate-b → Investigates theory B
└── teammate-c → Investigates theory C
    (teammates message each other to challenge/validate findings)
```

**Pattern 3: Cross-Layer Feature**
```
Leader creates tasks: api (T1), ui (T2 depends_on T1), tests (T3 depends_on T1,T2)
├── teammate-backend  → Claims T1
├── teammate-frontend → Waits for T1, then claims T2
└── teammate-tests    → Waits for T1+T2, then claims T3
```

**Pattern 4: Verification Teammate**
```
Leader creates implementation tasks + verification tasks (depends_on implementation)
├── teammate-core     → Claims T1 (implement module)
├── teammate-io       → Claims T2 (implement I/O layer)
└── teammate-verify   → Waits for T1,T2 → runs build, tests, reviews diffs
    ├── pass → message team-lead with verification report
    └── fail → message implementer directly with bug details,
               then message team-lead: "sent feedback to teammate-core on T1"
```

Verification teammate's scope:
- Run `build` / `test` commands (leader grants Bash access to this teammate)
- Compare completion reports against actual file diffs — flag discrepancies
- Check for cross-module inconsistencies (mismatched types, missing imports)
- Does NOT fix code — sends bug reports to the implementer, who fixes and re-reports

**Scaling:** A single verifier becomes a bottleneck at 3+ implementers. Split into two roles:

```
├── teammate-core     → Claims T1
├── teammate-io       → Claims T2
├── teammate-api      → Claims T3
├── verifier-build    → build/test runner — mechanical: compile, run tests, report pass/fail
└── verifier-review   → semantic reviewer — diff review, consistency check, contract validation
```

`verifier-build` runs immediately when any task completes (fast, parallel-safe). `verifier-review` runs after `verifier-build` passes (deeper, sequential). This prevents the build queue from blocking semantic review and vice versa.

For teams of 1-2 implementers, a single verifier is sufficient.

Spawn all three with `model="sonnet"` by default. Escalate `verifier-review` to Opus only if semantic review is missing regressions that cross-module reasoning would catch — and document that exception in its creation prompt.

Creation prompt examples:
```
# Single verifier (1-2 implementers)
"You are the verification teammate. Your role is to build, test, and review
the work of other teammates. You do NOT implement features. Wait for
implementation tasks to complete, then verify them. Report bugs directly
to the implementer. Report verification results to the leader."

# Split: build verifier (3+ implementers)
"You are the build verifier. Run 'just build' and 'just run' after each
implementation task completes. Report pass/fail to the leader and the
implementer. You do NOT review code semantics — that is verifier-review's job."

# Split: semantic verifier (3+ implementers)
"You are the semantic reviewer. After verifier-build passes, review the
implementer's diff against the task spec. Check for: missing parts, type
mismatches across modules, undocumented assumptions. Report issues directly
to the implementer, then notify the leader."
```

### Known Limitations

- **No session resume** — `/resume` and `/rewind` do not restore in-process teammates
- **Task status can lag** — teammates sometimes fail to mark tasks complete, blocking dependents. Leader should check and update manually if stuck
- **One implicit team per session** — there is no separate team to create or delete; teammates are spawned into it directly and shut down with `shutdown_request`
- **No nested teams** — teammates cannot create their own teams
- **Leader is fixed** — cannot transfer leadership

### Anti-Patterns

| Anti-Pattern | Problem | Fix |
|-------------|---------|-----|
| Using SendMessage for dependency coordination | Redundant; races with auto-unblock | Use `depends_on` in `workslate_task_create` |
| Overlapping file scope | Overwrites, lost work | One teammate per file |
| 6+ teammates | Coordination overhead dominates | Cap at 5 |
| Leader dispatches every task manually | Leader bottleneck, teammates idle | Let teammates self-claim; leader designs task graph |
| Leader skips build | Integration issues found late | Build immediately after completion |
| Broadcasting routine updates | Token waste | Use direct messages |
| Vague creation prompts | Wrong guesses | Include role, scope, file list |
| Task instructions in creation prompt | Teammate starts before tasks exist | Role/scope only in creation prompt |
| Teammate claims shared/integration task | Architectural inconsistency | Leader reserves these (owner = leader) |
| Teammate claims out-of-scope task | File conflicts | CLAUDE.md scope rules + clear creation prompts |
| Teammate silently reduces task scope | Incomplete deliverable, downstream breakage | Task scope is non-negotiable; report concerns to leader before starting |
| Spawning a teammate without `run_in_background=true` and trying to steer it mid-task | A foreground `Agent` call runs to completion before returning — the leader cannot message it mid-turn | Spawn steerable teammates with `Agent(name=..., run_in_background=true, ...)`; foreground `Agent` is for fire-and-forget subagents |
| Spawning a teammate with `subagent_type="Explore"` (or another read-only type) for implementation work | The teammate cannot edit or write files, and silently fails every implementation task it claims | Use a write-capable type (e.g., `general-purpose`) for implementation; reserve read-only types for pure research roles |
| Looking for a `TeamCreate` / `TeamDelete` step | Those tools do not exist in this Claude Code version; `team_name` is deprecated and ignored | There is one implicit team — spawn teammates directly with `Agent(name=..., run_in_background=true, ...)`, shut them down with `shutdown_request` |
| Spawning a `general-purpose` subagent without surfacing it first | Delegates write-capable work silently, at a reasoning level the leader can't raise (no effort knob on `Agent`); failure mode is a silent wrong-interpretation baked into files | Surface/propose it ("this splits into 2 independent edits — want me to fan out subagents?") and proceed on agreement. Read-only types (`Explore`, `Plan`) are free — use them proactively |
| Creating an Agent Team because the scale criteria happen to match | Scale describes *when a team is viable*, not *when to spawn one* — the write-gate (*Delegation* above) is the user's agreement to a proposal | Surface/propose; spawn on agreement, regardless of scale |
| Reaching for `Workflow` by default because a task "looks parallel" | A run costs far more than in-session work, and the scale is the user's to choose | Default to in-session; surface/propose the workflow (+ rough cost), run on a current-turn opt-in — never on stale/inferred `ultracode` |
| A workflow `agent()` left on default model/effort | Silently downgrades to the agent-type default (e.g. `Explore`→`haiku`), so the delegate runs weak | Set `opts.model` and `opts.effort` explicitly on every non-trivial `agent()` |

## Workflow (the third delegation surface)

The **`Workflow`** tool runs a deterministic JavaScript script that orchestrates many subagents — fanning out (`parallel`), pipelining (`pipeline`), looping, and branching in code rather than by model judgment. The script holds the control flow and the intermediate results; your context gets back only the final synthesized answer. It runs in the background and notifies you on completion. Use it for **large, breadth-first, mechanical** work — a codebase-wide sweep, an N-file migration, a multi-source research question, a panel of independent reviewers — where a single conversation would drown in intermediate output.

The `Workflow` *tool* is unrelated to this manual's "Three-Phase Workflow" (Understand/Plan/Execute) and "Leader Workflow" headings; always code-format `Workflow` when you mean the tool.

### Workflow is a real capability, but not the default

You *can* invoke `Workflow`. By default you do **not** — single-session work is the default — because a run's token cost is high (a single run can spend far more than doing the same work in conversation) and the *scale* is the user's to choose. Treat it like any write-capable delegation: **surface/propose → run on the user's agreement**. That agreement may arrive as any of:

- `ultracode` confirmed for the **current turn** by a system-reminder (the keyword in the user's prompt, or session mode on). The bare word "ultracode" sitting in transcript history, docs, a question ("what is ultracode?"), or a negation does **not** count — only the harness-confirmed signal.
- The user asking for a workflow in their own words ("use a workflow", "fan out agents", "orchestrate this"), or to run a named/saved workflow.
- A skill or command whose instructions call `Workflow` — but only when the **user** invoked that skill/command. An auto-selected skill must not smuggle in a `Workflow` call.
- The user agreeing to a workflow **you** proposed (mechanism + rough cost + what it will touch).

Absent a current-turn signal, default to **not** running one: do the work in-session, or surface/propose it. Never fire a workflow off stale or inferred opt-in.

### Set model and effort explicitly

A `Workflow` `agent()` does **not** reliably inherit the session's model: with a custom `agentType` it follows that type's own default, which can be a weak/cheap model (e.g. `agentType: 'Explore'` → `haiku`), silently downgrading the agent. On every `agent()` doing non-trivial work, set `opts.model` to the intended tier **and** `opts.effort` to the intended level. A silently-downgraded workflow agent is the same failure as a default-effort `Agent` delegate — the control knobs only help if you set them.

### `ultracode` raises thoroughness — it does not loosen the rules

When a system-reminder confirms `ultracode`, the standing posture is to author and run a workflow for substantive tasks and to favor exhaustiveness; multi-phase work becomes several workflows in sequence (understand → design → implement → review) with you in the loop between them. It does **not**:

- **Collapse the approval gate.** A read-only planning/design workflow may run once the workflow itself is opted-in, but a **write-capable implementation workflow still waits for the user's approval** to implement, exactly as in-session implementation does.
- **License scope reduction.** A workflow that would shrink, defer, or deviate from the approved scope stops and re-requests approval (`claude-agent-kit--task-execution.md`).
- **Substitute for your verification.** A workflow's internal adversarial-verify stages are good practice, not a substitute for you independently running the build/test on the synthesized result and reporting faithfully.
- **Make `budget()` semantic.** Budget exhaustion is not completion and not license to defer — stop, report the remaining scope, and ask.

### Quality flow

- **Pipeline by default** (`pipeline`); reach for a barrier (`parallel` between stages) only when a stage genuinely needs all prior results (dedup/merge, zero-count early-exit).
- **Verify before trusting a finding.** Independent/heterogeneous reviewers catch failure modes that redundant identical ones don't — homogeneous "debate" underperforms a plain majority vote, and extra rounds entrench errors. Diversify the lens, don't just add rounds.
- **One writer per final target file** carries into workflows (full rationale in *Delegation* above): parallel writers use `isolation: 'worktree'` and **you own the merge**; shared contracts/types stay leader-owned.
- **One well-scoped fan-out per workflow.** Read each result and decide the next phase yourself; don't fold understand → design → implement → review into one mega-run.
- **Guard loops on `budget.total`** for "+Nk"-style directives, and **`log()` any silent cap** (top-N, sampling, no-retry) so truncation never reads as full coverage.
- **Self-contained agent prompts** — workflow subagents don't inherit your conversation.
- **No nested orchestration** — a workflow subagent does not spawn its own `Workflow`/`Agent` orchestration (the harness throws on nested `workflow()`); you remain the single integration + verification owner.

### Code staging inside a workflow

Workflow subagents editing files in isolated worktrees use direct `Edit`/`Write` — the workslate staging discipline is impractical across parallel writers (shared SQLite, one-buffer-per-file). This is a **reasoned exception, not a license to skip review**: the review step is supplied by the workflow's own verify stages **plus** your post-hoc diff review of the synthesized output. The chain-of-thought-in-comments ban and the scope-integrity rules still bind workflow agents; and when **you** integrate a workflow's output into the working tree, you follow the normal workslate staging discipline for non-trivial merges.

### aside / advisor inside a workflow

**Workflow subagents do not call aside or `advisor()`.** Reasons, in order of firmness: (a) **cost** — N parallel agents each firing paid third-party aside calls is unbounded quota burn; (b) **coherence** — a second opinion inside a workflow belongs in the workflow's own judge/verify stages, not in scattered consultations; (c) the conservative stdio-transport concurrency hazard that `claude-agent-kit--aside.md` documents. You (the leader) own aside/`advisor()` and run them strictly serialized, per that file.

