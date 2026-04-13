<!-- claude-agent-kit -->
# Parallel Work

Two systems exist for parallel work: **Subagents** and **Agent Teams**. They have different architectures and should not be mixed.

## Choosing Between Subagents and Agent Teams

| | Subagents (`Agent` tool) | Agent Teams (`TeamCreate`) |
|---|---|---|
| Communication | Results returned to parent only | Teammates message each other directly |
| Coordination | Parent manages everything | Shared task list with self-claiming |
| Context | Own window; result summarized to parent | Own window; loads CLAUDE.md, MCP, skills |
| Task system | None (prompt = task) | `workslate_task_*` with dependencies + SQLite WAL concurrency |
| Best for | Focused, fire-and-forget work | Complex work requiring collaboration |
| Token cost | Lower | Higher (each teammate is a full Claude instance) |

**Decision rule:** Workers need to communicate with each other? Agent Teams. Just do independent work and report back? Subagents.

## Subagents

Lightweight workers spawned via the `Agent` tool. Execute a task and return a result — no inter-agent communication.

**Prompt rules:**
- Prompts must be **self-contained** — include all necessary context inline
- Subagents load CLAUDE.md but do not inherit the parent's conversation history
- Specify exact file paths and expected outputs
- State what the subagent should NOT do

**When to use:**
- Research/exploration that reports back findings
- Independent file modifications with no cross-dependencies
- Build/test verification
- 1-2 parallel tasks where team overhead is not justified

**Naming:** `agent-<domain>` (e.g., `agent-vfs`, `agent-core`)

## Agent Teams

A coordination system for multiple Claude Code instances that work together via shared task lists and direct messaging. **Experimental feature** — requires `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`.

### Agent Teams are expensive — do not reach for them first

Each teammate is a full Claude Code instance. On spawn, each teammate independently loads CLAUDE.md, every MCP server, and every skill. Once running, every completion report, idle notification, and status update flows through the leader's context. A 5-teammate team spends roughly 3–5× the tokens of the same work done in a single session.

**Scale criteria — use these, not "the work feels parallel":**

| Scope | Recommended approach |
|---|---|
| < 5 files to modify | Single session. No team, no subagents. |
| 5–10 files, cross-cutting concerns | Leader session + 1–2 subagents. Subagents do research or isolated edits; leader integrates. |
| 10+ files with clean, non-overlapping file scopes | Agent Team is justified. |
| 10+ files but scopes overlap / shared types dominate | Still single session — a team will generate coordination overhead that exceeds the parallelism win. |

**When unsure: do not create a team.** A slower single session is cheaper than a fast-but-expensive team. The user can always ask for parallelism if they want it.

### Model choice for teammates

Teammates are spawned via the `Agent` tool, which takes a `model` parameter (`sonnet` | `opus` | `haiku`). Pick deliberately — model choice is the single biggest lever on team cost after team size.

- **Default teammates to Sonnet** — `Agent(team_name=..., name=..., model="sonnet", ...)`. Teammate work is well-scoped: claim an unblocked task, edit files inside an assigned scope, produce a completion report. Sonnet handles this reliably at a fraction of Opus token cost, and the leader (on Opus) is where cross-teammate reasoning happens anyway.
- **Leader stays on Opus** — inherited from the current session, no override needed. The leader designs the task graph, reconciles conflicting assumptions between teammates, and owns integration/verification. Weakening the leader to save tokens usually costs more in rework.
- **Escalate a specific teammate to Opus only for genuine reasoning load** — e.g., a `verifier-review` / semantic reviewer that must catch subtle contract mismatches across modules, or an `arch-designer` making cross-cutting design calls. Note the exception in the creation prompt so future readers know why that teammate is not on the default.
- **Model choice does not license scope shrinkage.** Sonnet teammates are still bound by the "task scope is non-negotiable" rule — if a Sonnet teammate cannot complete the task as specified, they report to the leader rather than silently trimming it.

**How Agent Teams actually work (system-level guarantees):**
- Teammates load **CLAUDE.md, MCP servers, and skills** automatically (same as any Claude Code session)
- Teammates do NOT inherit the leader's conversation history
- The shared task list supports **self-claiming** with file locking to prevent races
- Task dependencies resolve **automatically** — when a blocking task completes, dependent tasks unblock without manual intervention
- Messages between teammates are delivered **automatically** (no polling)
- Task assignment via `TaskUpdate` is delivered to the teammate automatically by the system

### When to Use

**Use Agent Teams when:**
- 3+ independent work streams can run in parallel
- Teammates need to share findings or challenge each other
- Work requires discussion and collaboration (competing hypotheses, cross-layer changes)

**Do NOT use when:**
- Work is sequential (each step depends on the previous)
- Only 1-2 files need modification
- Workers do not need to communicate (use subagents instead)

### Team Composition

**Naming:**

| Element | Convention | Example |
|---------|-----------|---------|
| Team name | kebab-case, describes objective | `auth-refactor` |
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
1. workslate_task_init → Create a named session for this team effort
2. TeamCreate          → Team + teammates created
                          (teammates explore codebase while waiting — see Creation Prompt below)
3. workslate_task_create(namespace="team") → Design task graph with depends_on and owner
4. Teammates work      → Self-claim eligible tasks via workslate_task_update(owner=self)
5. Monitor             → Footer shows team progress; intervene only when stuck
6. Build & verify      → After all teammates complete
7. Fix integration     → Missing imports, visibility, mod declarations
8. Shutdown            → shutdown_request to each teammate
9. TeamDelete          → Clean up team resources
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
2. **Design the task graph** — proper scope, `blockedBy` dependencies, leader-reserved marking
3. Mark shared types / integration / cross-scope tasks as **leader-reserved** (assign owner to leader via `TaskUpdate`)
4. **Run build & tests** — teammates may lack Bash permissions
5. Fix integration issues after all teammates complete
6. Shutdown all teammates before `TeamDelete`

**Leader checklist:**
- [ ] Teammates spawned with `model="sonnet"` unless a specific role justifies Opus (document the exception in the creation prompt)
- [ ] Creation prompts contain role/scope only (no implementation instructions)
- [ ] Task graph designed with proper `blockedBy` dependencies
- [ ] Shared types / integration / public interface tasks reserved to leader (owner = leader)
- [ ] Each teammate's file scope does not overlap
- [ ] Build executed after teammates report completion
- [ ] All teammates shut down before cleanup

### Leader Intervention

The leader must actively monitor, not just wait for completion reports. Intervene when:

- **Inconsistent assumptions** — multiple teammates' reports reveal conflicting interpretations of shared types, APIs, or contracts. Fix: pause affected teammates, clarify the contract, then resume.
- **Silent stall** — a task is unblocked and claimed but no progress or report arrives. The teammate may be stuck without recognizing it as a blocker. Fix: message the teammate to check status.
- **Downstream failure** — a task completes but dependent tasks fail to proceed or produce unexpected results. The upstream output may be subtly wrong. Fix: review the completed task's output before letting dependents continue.
- **Scope drift** — a completion report shows files modified outside the teammate's assigned scope. Fix: revert or reassign, reinforce scope boundaries.
- **Duplicated work** — two teammates produce overlapping implementations (e.g., both define the same helper type). Fix: choose one, remove the other, update task graph.

The leader does NOT need to review every completion report in detail. Skim reports for red flags (unexpected files, cross-module references outside scope, ambiguous contract descriptions) and investigate only those.

### Teammate Behavior

> **Teammates read this section directly** — CLAUDE.md is loaded by all teammates.

**When you are a teammate in an Agent Team, follow this work loop:**

1. **On creation:** Read and explore code within your assigned scope. **Do NOT start implementing anything.** Wait until tasks appear in the task list.
2. **Self-claim** an eligible task (see Task Claiming Policy below).
3. **Work** on that task only. Stay within your assigned file scope.
4. **On task completion:** Send a **completion report** to the leader using the format below, then self-claim the next eligible task. If no eligible task exists, wait.
5. **On blocker:** Report to the leader immediately and wait.
6. **On `shutdown_request`:** Finish current work and shut down gracefully.

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
1. The task is **unblocked** (all `blockedBy` dependencies completed)
2. The task is **unassigned** (no owner set)
3. The task is **within the teammate's assigned file scope**
4. The task does **NOT** modify shared files, shared types, or public interfaces

**Tasks the leader must handle directly (leader-reserved):**
- Shared types / constants / public interface definitions
- Integration and final wiring tasks
- Cross-scope tasks that touch multiple teammates' files
- Any task where ownership is ambiguous

Leaders mark these as reserved by assigning owner to themselves via `TaskUpdate`. Teammates must not claim tasks that already have an owner.

**When multiple eligible tasks are available, prioritize in this order:**
1. Tasks on the **critical path** — tasks that other tasks are `blockedBy` (unblocking others has the highest throughput impact)
2. Tasks with the **most dependents** — prefer unblocking 3 teammates over unblocking 1
3. Tasks **relevant to current work context** — minimize context switching as a tiebreaker

### File Conflict Prevention

**Cardinal rule: no two teammates modify the same file.**

- Each teammate's file scope is defined in their creation prompt
- Shared dependencies (types, constants) get their own task; other tasks depend on it via `blockedBy`
- If two teammates must touch the same file, assign it to exactly one

### Communication

| Situation | Method | Notes |
|-----------|--------|-------|
| Task completion | `message` to leader | Include completion report |
| Sharing findings | `message` to specific teammate | Direct teammate-to-teammate |
| Blocker | `message` to leader | Immediate |
| Critical issue | `broadcast` | Rarely — cost scales with team size |
| Shutdown | leader sends `shutdown_request` | After confirming completion |
| Verification fail | `message` to implementer + leader | Verifier reports bug to implementer directly, notifies leader that feedback was sent |
| Verification pass | `message` to leader | Verifier confirms build/test clean |

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
Leader creates tasks: types (T1), core (T2 blockedBy T1), io (T3 blockedBy T1), misc (T4)
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
Leader creates tasks: api (T1), ui (T2 blockedBy T1), tests (T3 blockedBy T1,T2)
├── teammate-backend  → Claims T1
├── teammate-frontend → Waits for T1, then claims T2
└── teammate-tests    → Waits for T1+T2, then claims T3
```

**Pattern 4: Verification Teammate**
```
Leader creates implementation tasks + verification tasks (blockedBy implementation)
├── teammate-core     → Claims T1 (implement module)
├── teammate-io       → Claims T2 (implement I/O layer)
└── teammate-verify   → Waits for T1,T2 → runs build, tests, reviews diffs
    ├── pass → message leader with verification report
    └── fail → message implementer directly with bug details,
               then message leader: "sent feedback to teammate-core on T1"
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
- **One team per session** — clean up before starting a new team
- **No nested teams** — teammates cannot create their own teams
- **Leader is fixed** — cannot transfer leadership

### Anti-Patterns

| Anti-Pattern | Problem | Fix |
|-------------|---------|-----|
| Using SendMessage for dependency coordination | Redundant; races with auto-unblock | Use `blockedBy` in TaskCreate |
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
