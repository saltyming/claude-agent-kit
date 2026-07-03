<!-- slate-agent-kit:common -->
# The Delegation Loop

When work fans out to delegates: **gate → select mechanism → integrate →
verify**. This file owns GATE-DELEGATE and the procedures behind INV-GATE-1/2/3
(defined in `CLAUDE.md`).

## Delegate taxonomy — and where aside/dispatch sit

- **Read-only delegates** inspect, search, plan, review, or summarize and return
  results without mutating files or external state.
- **Write-capable delegates** can edit files, run write-capable tools, or mutate
  state.
- **Fan-out helpers** run the same role or prompt over multiple inputs and
  aggregate results.
- **Skills / commands / workflows** are helper surfaces. They do not bypass
  scope, approval, or verification rules.

Two MCP-backed surfaces sit alongside the harness-native ones, and this is the
one place their relationship is stated: **`aside`** (`claude-agent-kit--aside.md`) is
*horizontal* consultation — a read-only cross-family second opinion; you stay in
charge. **`dispatch`** (`claude-agent-kit--dispatch.md`) is *hierarchical* delegation —
it hands a **write-capable** execution step to an external coding agent and
tracks it asynchronously. A read-only opinion is aside; entrusting a build/edit
step to an external agent is dispatch, which carries its own approval gate
(GATE-DISPATCH) and server-enforced guards.

If a harness lacks a mechanism needed for safe delegation, surface that gap to
the user rather than improvising an unsafe substitute.

## Read-only delegates are free — use them proactively

Read-only delegates read widely and return only a summary — they *reduce* the leader's context cost. Reach for these without asking; they are **not** gated (INV-GATE-1).

For read-only lookups larger than ~3 search queries, prefer a subagent over inline search — the leader's context is the bottleneck, and a subagent returns the result, not the evidence.

## GATE-DELEGATE: write-capable delegation (HARD RULE)

**GATE-DELEGATE — the procedure for INV-GATE-1.** Any write-capable delegate can edit files or mutate state. The delegated result is what you see, not the reasoning; a wrong choice becomes a committed mistake. The user owns that cost, so the agent owns the proposal:

**For every write-capable delegate, surface first, spawn on agreement:**

- The **mechanism** — for example, a harness worker subagent, a team lane, a fan-out job, or a dispatch execution step.
- The **rough cost / scale** — "single subagent, ~5 files touched".
- **The files it will write** — `src/auth/login.ts`, `src/auth/session.ts`, … .

Spawn only after explicit agreement. Generic "just go" against an earlier ambiguous phrasing does not count — re-propose.

**The gate is on capability, not on the prompt.** Even if you intend to send a read-only prompt to a write-capable delegate, the gate still applies — because the delegate *can* edit, a prompt-level "just read" is not a barrier. Use a genuinely read-only delegate for read-only work; do not work around the gate by changing prompt wording. Default for an unknown/ambiguous delegate type: treat as write-capable.

**Mechanism-specific carve-out:** a harness adapter may carry an explicit user-configured auto-approval policy for one mechanism (e.g. dispatch's `proactive` + `auto` execution policy) — that policy then governs that mechanism instead of this gate. Nothing else loosens it.

## Selection by dependency structure

After the gate is agreed, pick the surface by what the work looks like, not by default:

- **Independent, non-overlapping subtasks, contract-stable** → parallel write-capable delegates.
- **Same prompt, many inputs** (review N files, classify N items) → a fan-out helper, if the prompt template is tight.
- **Sequential or tightly-coupled work** → in-session. Don't fan out subtasks that have to happen in order.
- **A subtask needs a different role** (research, design) → a role-specific read-only or advisory delegate for that one slot.
- **An isolated, self-contained execution step** (mechanical edits, long verification loops, well-scoped sweeps) → `dispatch` to an external backend, under its own policy.

Before calling any split "independent", identify shared contracts first — public types, schemas, migration ordering, shared tests, invariants — and keep those leader-owned. Independent-looking files often share a contract.

## One writer per final target file (INV-GATE-2 procedure)

No two delegates edit the same file. If two subagents would touch the same file, assign it to exactly one (or one explicit owner the leader integrates after). Otherwise the merge is the bottleneck. Worktree-style isolation prevents disk clobber but not divergent edits — the rule holds regardless.

## Delegated children inherit the invariants (INV-GATE-3 procedure)

A delegate that hits a forced spec/plan deviation stops and reports to the leader, who then asks the user — a child never shrinks scope, reinterprets a budget, or substitutes a cleaner design on its own (GATE-DEVIATION in `claude-agent-kit--task-execution.md`). When delegating a palette story, hand the delegate the *approved, tracked scope*, never the raw Tier-A artifact (`claude-agent-kit--palette.md` § Gate bindings).

## Prompt rules for delegates

- **Self-contained** — include all necessary context inline. Subagents do not inherit the parent's conversation history.
- **Specific paths and expected outputs** — name files, name what success looks like.
- **State what the delegate should NOT do** — explicit out-of-scope cuts down on re-litigating settled decisions.
- **Name the operating envelope.** A delegate sees only its prompt, so the durability bar (INV-QUALITY-1) must be stated, not assumed: name the platforms, harnesses, input classes, and callers the change must hold under, and require the cause fixed rather than the symptom patched. Delegates optimize for making the immediate step pass — the envelope in the brief is what stands between you and a works-on-my-case patch.
- **Frame settled decisions as constrained choices, not open questions.** A subtask that already carries a decision you made should be passed in framed ("do A; use B only if X; don't introduce a third pattern"), not as an open prompt ("figure out how to handle Y"). Open prompts re-open design space you've already settled — burning tokens and risking divergence from what sibling lanes assume.
- **A delegate can't watch the live output of a backgrounded, long-running, or streaming process.** Design such commands around one of three escapes, and name the chosen one in the prompt: (1) **write-then-read** — redirect to a log file and read it back; (2) **observability sink** — a structured results file / status endpoint the delegate queries; (3) **output-free success check** — an artifact or exit-code file that decides success without a stream.

## When unsure, do the work in-session and propose

A slower in-session edit beats a fast-but-silently-wrong delegated one. If parallelism would genuinely help, surface it ("this splits into N independent edits — want me to fan out subagents, ~rough cost?") and proceed on agreement.

## Anti-patterns

- **Spawning a write-capable delegate silently** — bypasses GATE-DELEGATE. The result is durable on disk; the user must have agreed.
- **Assigning implementation work to a read-only delegate** — it cannot edit files. Silent failure.
- **Two delegates editing the same file** — manual merge pain, lost work (INV-GATE-2).
- **Fan-out over a too-coarse prompt** — a 12-item sweep with "general code reviewer" returns 12 low-signal summaries. Tighten the prompt template, or run as in-session review.
- **Reaching for delegates when in-session is faster** — leader context is the goal, but coordination overhead is a real cost for tasks smaller than a few files. The scale is the user's, not yours.

---

## Claude delegation surfaces

Three mechanisms parallelize work in Claude Code — two via the `Agent` tool, one via the separate **`Workflow`** tool:

- **Subagents** — `Agent` fire-and-forget: runs once, returns a result.
- **Teammates** — `Agent` **named** + run **in the background** (`run_in_background: true`): a persistent peer that keeps running, shares the task list, and exchanges messages with other teammates. Subagents and teammates both spawn through the same `Agent` tool into a **single implicit team** (no `TeamCreate`; `team_name` is deprecated/ignored) — the difference is lifecycle, not a separate system.
- **`Workflow`** — a *separate tool* that runs a deterministic script orchestrating many subagents (fan-out / pipeline) in the background; for large, breadth-first work. It is **not** a `subagent_type`. See *Workflow* below.

The fourth surface, **`dispatch`**, is external execution delegation — covered by the shared taxonomy above and `claude-agent-kit--dispatch.md`.

**Effort/model control differs by surface.** The interactive `Agent` tool exposes `model` but no reasoning-effort knob — a delegated `Agent` runs at the CLI's default reasoning level, which the leader can't raise. The `Workflow` `agent()` *does* expose `effort` (and `model`) — set them explicitly (they don't reliably inherit). So a `Workflow` can be the more controllable surface for large fan-out once opted in.

### Choosing Between Subagents and Teammates

| | Subagents (`Agent`, fire-and-forget) | Teammates (`Agent` with `name` + `run_in_background`, single implicit team) |
|---|---|---|
| Communication | Results returned to parent only | Teammates message each other directly |
| Coordination | Parent manages everything | Shared task list with self-claiming |
| Context | Own window; result summarized to parent | Own window; loads CLAUDE.md, MCP, skills |
| Task system | None (prompt = task) | `workslate_task_*` with dependencies + SQLite WAL concurrency |
| Best for | **Independent** subtasks (no coordination, no mid-task steering) | **Dependent / coordinated** work (shared `depends_on`, cross-talk, mid-turn steering) |
| Token cost | Lower | Higher (each teammate is a full Claude instance) |

### Spawn mechanism

The `Agent` tool is the **spawn mechanism for both** subagents and teammates. The discriminator is **`run_in_background` + `name`**:

| Invocation | What you get |
|---|---|
| `Agent(subagent_type=…, prompt=…, …)` | A **subagent**. Fire-and-forget, own context window, no peer messaging. |
| `Agent(name=…, subagent_type=…, model=…, run_in_background=true, prompt=…)` | A **teammate** in the implicit team. Named (peers address it via `SendMessage`), backgrounded (runs across the leader's turns, steerable mid-task). Shares the task list, runs until `shutdown_request`. |

Key facts:

- **There is no team container to create.** The implicit team always exists; populate it by calling `Agent(name=…, run_in_background=true, …)` once per teammate. Add more mid-run with additional calls.
- **`run_in_background=true` is what makes a teammate steerable.** Without it, even a named `Agent` call runs to completion before returning.
- **`subagent_type` controls capability.** Read-only types (`Explore`, `Plan`) cannot edit files — never assign them implementation work. Use a full-capability type (`general-purpose`) for teammates that must modify code.
- **`model` controls cost.** Default teammates to `model="sonnet"`; escalate to Opus only where documented below.

### Subagents

**When to use (read-only types — proactive is fine):**
- `subagent_type="Explore"` for broad codebase research that would take more than ~3 Grep/Glob queries.
- `subagent_type="Plan"` for read-only design sketches / architecture exploration.
- Other advisory-only types (`claude-code-guide`, etc.) for their documented scope.

**When to use (write-capable types — GATE-DELEGATE applies):**
- `subagent_type="general-purpose"` for parallel implementation, build/test verification that writes files, or any delegated work that can edit or create files — when the subtasks are genuinely **independent**.

**Naming:** `agent-<domain>` (e.g., `agent-vfs`, `agent-core`).

### Agent Teams

A coordination system for multiple Claude Code instances that work together via shared task lists and direct messaging — the right tool for **dependent, coordinated** work. Requires `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` in settings/env; see *Known Limitations* below.

#### Agent Teams cost more — match them to coordinated work

Each teammate is a full Claude Code instance. On spawn, each teammate independently loads CLAUDE.md, every MCP server, and every skill. Once running, every completion report, idle notification, and status update flows through the leader's context. A 5-teammate team spends roughly 3–5× the tokens of the same work done in a single session.

**Scale criteria — use these, not "the work feels parallel":**

| Scope | Default — do in-session, or surface/propose | If the user agrees to the proposal |
|---|---|---|
| < 5 files to modify | Single session. No team, no subagents. | Single session even if asked — the overhead isn't worth it. Explain why. |
| 5–10 files, cross-cutting concerns | Single session. Optionally `Explore` subagents for read-only research. | Leader session + 1–2 `general-purpose` subagents for isolated edits; leader integrates. |
| 10+ files with clean, non-overlapping file scopes | Single session (propose parallelism if you think it would help). | Agent Team is justified. |
| 10+ files but scopes overlap / shared types dominate | Single session. | Still single session — coordination overhead exceeds the parallelism win. Tell the user. |

**Scale informs *what you propose*, not *whether* to spawn unprompted** (GATE-DELEGATE).

#### Model choice for teammates

- **Default teammates to Sonnet** — teammate work is well-scoped: claim an unblocked task, edit files inside an assigned scope, produce a completion report. Sonnet handles this reliably at a fraction of Opus token cost, and the leader is where cross-teammate reasoning happens anyway.
- **Leader stays on the session model** — the leader designs the task graph, reconciles conflicting assumptions, and owns integration/verification. Weakening the leader to save tokens usually costs more in rework.
- **Escalate a specific teammate to Opus only for genuine reasoning load** — e.g., a `verifier-review` semantic reviewer catching subtle contract mismatches across modules. Note the exception in the creation prompt.
- **Model choice does not license scope shrinkage** (INV-GATE-3).
- **Pick `subagent_type` to match the role.** Implementation teammates need `general-purpose`; a pure-research teammate can use `Explore`. When in doubt, `general-purpose` — the only built-in type that both reads and writes.

**How teammates work (system-level guarantees):** teammates load CLAUDE.md, MCP servers, and skills automatically; they do NOT inherit the leader's conversation history; built-in `SendMessage` delivers automatically at turn boundaries.

**Two task systems — keep them distinct:** Claude Code's built-in Agent-Team task list (`TaskCreate`/`TaskList`/`TaskUpdate`) is a separate system; **this project does NOT use it for team coordination**. `workslate_task_*` (`ws:`/`team:` namespaces, `depends_on`) is the coordination + tracking system — the one the doorbell footer surfaces on every tool call. Self-claim is *behavioral* (Task Claiming Policy below), and a teammate sees a new assignment via the doorbell.

#### When to use

Use Agent Teams when ALL hold: the user agreed to the proposal (GATE-DELEGATE); 3+ independent work streams can run in parallel; teammates need to share findings or challenge each other; the work requires discussion (competing hypotheses, cross-layer changes). Do NOT use for sequential work, 1–2 file changes, or workers that never need to communicate (use subagents).

#### Team composition

| Element | Convention | Example |
|---------|-----------|---------|
| Leader (you) | the reserved role `team-lead` — register under it on startup | `team-lead` |
| Teammates | descriptive role name | `security-reviewer`, `arch-designer` |

**Team size:** 3–5 teammates. 5–6 tasks per teammate keeps everyone productive.

**Task granularity:** each task produces a clear, self-contained deliverable (a module, a test file, a handler). Too small = coordination overhead exceeds benefit; too large = self-claiming loses meaning. A good task takes a teammate roughly 5–15 minutes of focused work.

#### Leader workflow

The leader is **task graph architect + build/integration owner**, not task dispatcher. `ws:` = leader's personal phases; `team:` = team assignments with owner and dependencies; the footer shows both (`ws:[2/4] team:[8/12]`).

```
1. workslate_task_init(<name>, session_id=<S>)   → named session (session_id from the SessionStart hint)
2. workslate_register(role="team-lead", session_id=<S>, agent_id="")
                                                 → register yourself (agent_id="" IS the main session);
                                                   then workslate_inbox_read(role="team-lead")
3. Agent(name=…, subagent_type=…, model="sonnet", run_in_background=true, prompt=<role-only>)
                                                 → spawn each teammate; they explore their scope while waiting
4. workslate_task_create(namespace="team")       → design task graph with depends_on and owner
5. Teammates work                                → self-claim eligible tasks via workslate_task_update(owner=self)
6. Monitor                                       → footer shows team progress; intervene only when stuck
7. Build & verify                                → after all teammates complete
8. Fix integration                               → missing imports, visibility, mod declarations
9. Shutdown                                      → shutdown_request to each teammate
```

**Creation prompts describe role and file scope only — never specific tasks** (a teammate with tasks in its prompt starts implementing before the task graph exists):

```
# Good — role + scope, no actionable work
"You are the auth module teammate. Your scope is src/auth/.
Read and understand the code in your scope while waiting for task assignments."
```

**Leader checklist:**
- [ ] Registered as `team-lead` so teammate messages reach you and the inbox doorbell fires
- [ ] Teammates on `model="sonnet"` unless a documented exception
- [ ] Implementation teammates on a write-capable `subagent_type` — never `Explore`/`Plan`
- [ ] Creation prompts contain role/scope only
- [ ] Task graph has proper `depends_on`; shared types / integration / cross-scope tasks reserved to leader (owner = leader)
- [ ] Teammate file scopes do not overlap (INV-GATE-2)
- [ ] Build executed after teammates report completion
- [ ] All teammates shut down (`shutdown_request`) when done
- [ ] Discovered `GOTCHA`s routed: durable → native memory; run-local → sibling prompts or `workslate_msg_send`

#### Leader intervention

Actively monitor; intervene when: **inconsistent assumptions** across reports (pause, clarify the contract, resume); **silent stall** on a claimed task (message the teammate); **downstream failure** after an upstream "completion" (review the upstream output); **scope drift** (revert or reassign); **duplicated work** (choose one, update the graph). Skim reports for red flags; investigate only those.

**User-initiated teammate interrupt — do NOT intervene.** When a teammate stops in a way that did **not** come from (a) your `shutdown_request`, (b) normal task completion, or (c) an explicit error/blocker report, assume the user acted on that teammate directly. Do not re-assign the task, pivot, spawn a replacement, or infer what the teammate "would have done." Surface "waiting for user direction — teammate X interrupted, task Y held" and wait. If you cannot tell a user interrupt from a crash, ask before any recovery action.

#### Teammate behavior

**When you are a teammate, follow this loop:**

1. **On creation:** read/explore your assigned scope. Do NOT start implementing. Wait for tasks.
2. **Self-claim** an eligible task (Task Claiming Policy below).
3. **Work** on that task only, within your file scope.
4. **On completion:** send a completion report to `team-lead` (format below), then claim the next eligible task.
5. **On blocker:** report to `team-lead` immediately and wait.
6. **On `shutdown_request`:** finish current work, shut down gracefully.
7. **Before idling:** as your final action, `workslate_inbox_read(role=<your name>)` — the doorbell only fires while you run tools; drain the inbox before you stop making tool calls.

Rules: do not run build/test directly (ask the leader); never touch files outside your scope; ask on ambiguous ownership; never reduce task scope (INV-GATE-3); notify affected teammates directly when your output (types, APIs, formats) feeds their tasks.

#### Completion report format (HARD RULE)

Plain text, under ~500 tokens, exactly this structure:

```
TASK: <id> — DONE

CHANGED:
- <file:line-range>: <1-line summary>

VERIFICATION:
- <grep check, invariant confirmed, types compile — concrete evidence>

DEFERRED (optional, omit if none):
- <thing intentionally not touched and why>

GOTCHA (optional — a distilled one-line trap; include ONLY if it should change a sibling prompt, a future decomposition, or native memory):
- <trap → how to avoid, 1 line>

NEXT: <ready for task X / shutdown / blocked on Y>
```

No process narration, no per-hunk description, no pasted code. `VERIFICATION` carries **concrete evidence** ("grep 'fn old_name' returns 0 matches"), not assertions.

#### Task claiming policy

Teammates may self-claim a task when ALL hold: unblocked (deps complete), unassigned, within their file scope, and NOT touching shared files/types/public interfaces. **Leader-reserved:** shared types/constants/interfaces, integration/final wiring, cross-scope tasks, ambiguous ownership — the leader assigns itself as owner. Priority when multiple are eligible: critical-path tasks → most dependents → context-relevant.

#### Mid-turn steering & team messaging (workslate)

Built-in `SendMessage` delivers only at a teammate's next turn boundary. workslate adds a **per-tool-call doorbell** to close the gap:

- `workslate_register(role, session_id, agent_id)` — map this session to a role. A subagent shares its parent's `session_id`, so `agent_id` (from the `SubagentStart` hint) is what distinguishes agents; both must come from the hint.
- `workslate_msg_send(recipient, subject, body, urgent?, session_id?, agent_id?)` — durable message to a role's inbox; `subject` shows in the doorbell.
- `workslate_inbox_read(role)` — return unread messages and mark them read (atomic).

**Teammate startup (required):** from the `SubagentStart` hint `[workslate] agent_id=<A> session_id=<S>`: `workslate_task_init(<leader's session name>, session_id=<S>, agent_id=<A>)` → `workslate_register(role=<your name>, session_id=<S>, agent_id=<A>)` → `workslate_inbox_read(role=<your name>)`. The leader propagates the task-session name in the creation prompt.

**Leader startup (required):** `workslate_task_init(<name>, session_id=<S>)` → `workslate_register(role="team-lead", session_id=<S>, agent_id="")` → `workslate_inbox_read(role="team-lead")`. `agent_id=""` IS the main session's identity.

**HARD RULE — every `msg_send` passes BOTH `session_id` and `agent_id`.** Leader and teammates share one `session_id`; `agent_id` is the only discriminator, and the leader owns the `(session_id, "")` slot. `msg_send` rejects a call that has a session in effect but omits `agent_id` — without the guard it would collide with the leader's row and mis-attribute the sender. `workslate_register` enforces the same guard (its `ON CONFLICT` overwrites `role` — a teammate omitting `agent_id` would clobber `team-lead`). `task_init` intentionally is NOT guarded (it preserves `role` on conflict and is also called by solo sessions).

**Addressing is by role, not session** — a respawned teammate of the same role still receives prior messages. One teammate per role per task session.

**Steering a teammate that may already be idle — send BOTH channels:** (a) `workslate_msg_send(recipient=<role>, …, urgent=true, session_id=<S>, agent_id="")` — the durable content; **and** (b) a built-in `SendMessage` telling it to run `workslate_inbox_read(role=<its role>)` — the wake-up. For a still-running teammate the doorbell + `urgent` suffice.

#### Communication

| Situation | Method | Notes |
|-----------|--------|-------|
| Task completion | `message` to `team-lead` | Include completion report |
| Sharing findings | `message` to specific teammate | Direct teammate-to-teammate |
| Blocker | `message` to `team-lead` | Immediate |
| Critical issue | `broadcast` | Rarely — cost scales with team size |
| Shutdown | leader sends `shutdown_request` | After confirming completion |
| Verification fail | `message` to implementer + `team-lead` | Verifier reports the bug to the implementer directly |
| Verification pass | `message` to `team-lead` | Verifier confirms build/test clean |

Teammate-to-teammate triggers (you MUST message directly): your output defines types/constants/APIs another teammate's task consumes → send the signatures/paths; you find a bug or assumption conflict in completed work → message them, then inform the leader; your deliverable changed shape from plan → message all dependents. Refer to teammates by name; plain text only; do NOT use SendMessage to coordinate task dependencies — the task system handles this.

#### Common patterns

- **Parallel module decomposition:** types (T1), core (T2 dep T1), io (T3 dep T1), misc (T4) — teammates claim as dependencies unblock.
- **Competing hypotheses:** one investigation task per theory; teammates challenge each other's findings directly.
- **Cross-layer feature:** api (T1), ui (T2 dep T1), tests (T3 dep T1,T2).
- **Verification teammate:** implementation tasks + verification tasks (dep implementation). The verifier runs build/test, compares reports against diffs, checks cross-module consistency; does NOT fix code — reports bugs to the implementer. At 3+ implementers, split `verifier-build` (mechanical pass/fail, runs immediately) from `verifier-review` (semantic diff review, runs after build passes). All on Sonnet; escalate `verifier-review` to Opus only if it demonstrably misses cross-module regressions.

#### Known limitations

- No session resume — `/resume` / `/rewind` do not restore in-process teammates.
- Task status can lag — the leader checks and updates manually when stuck.
- One implicit team per session; no nested teams; the leader is fixed.

#### Claude-specific anti-patterns

| Anti-pattern | Problem | Fix |
|-------------|---------|-----|
| SendMessage for dependency coordination | Redundant; races with auto-unblock | `depends_on` in `workslate_task_create` |
| 6+ teammates | Coordination overhead dominates | Cap at 5 |
| Leader dispatches every task manually | Leader bottleneck, teammates idle | Self-claiming; leader designs the graph |
| Leader skips build | Integration issues found late | Build immediately after completion |
| Broadcasting routine updates | Token waste | Direct messages |
| Task instructions in creation prompt | Teammate starts before tasks exist | Role/scope only |
| Teammate claims shared/integration task | Architectural inconsistency | Leader reserves these |
| Spawning a teammate without `run_in_background=true` and trying to steer it | A foreground `Agent` call runs to completion first | Background + name for steerable teammates |
| `Explore`/`Plan` teammate for implementation | Cannot edit files; silent failure | `general-purpose` for implementation |
| Looking for `TeamCreate`/`TeamDelete` | They do not exist; `team_name` is ignored | One implicit team; spawn directly, `shutdown_request` to end |
| A workflow `agent()` left on default model/effort | Silently downgrades to the agent-type default (e.g. `Explore`→haiku) | Set `opts.model` and `opts.effort` explicitly |

### Workflow (the third delegation surface)

The **`Workflow`** tool runs a deterministic JavaScript script that orchestrates many subagents — fanning out (`parallel`), pipelining (`pipeline`), looping, and branching in code rather than by model judgment. The script holds the control flow and intermediate results; your context gets back only the final synthesized answer. Use it for **large, breadth-first, mechanical** work — a codebase-wide sweep, an N-file migration, a multi-source research question, a panel of independent reviewers.

#### Workflow is a real capability, but not the default

A run's token cost is high and the *scale* is the user's to choose. Treat it like any write-capable delegation: **surface/propose → run on the user's agreement**. That agreement may arrive as any of:

- `ultracode` confirmed for the **current turn** by a system-reminder (the keyword in the user's prompt, or session mode on). The bare word in transcript history, docs, a question, or a negation does **not** count.
- The user asking for a workflow in their own words ("use a workflow", "fan out agents", "orchestrate this"), or to run a named/saved workflow.
- A skill or command whose instructions call `Workflow` — but only when the **user** invoked that skill/command.
- The user agreeing to a workflow **you** proposed (mechanism + rough cost + what it will touch).

Absent a current-turn signal, do the work in-session or surface/propose. Never fire a workflow off stale or inferred opt-in.

#### `ultracode` raises thoroughness — it does not loosen the rules

When confirmed, the posture is to author and run workflows for substantive tasks and favor exhaustiveness, multi-phase (understand → design → implement → review) with you in the loop between runs. It does **NOT**: collapse the approval gate (a write-capable implementation workflow still waits for the user's approval to implement); license scope reduction (GATE-DEVIATION applies inside workflows too); substitute for your own verification of the synthesized result (INV-VERIFY-1); or make `budget()` semantic — budget exhaustion is not completion; stop, report the remaining scope, ask.

#### Quality flow

- **Pipeline by default**; a barrier (`parallel` between stages) only when a stage genuinely needs all prior results (dedup/merge, zero-count early-exit).
- **Verify before trusting a finding.** Diversify reviewer lenses instead of adding identical rounds — homogeneous "debate" underperforms a plain majority vote.
- **One writer per final target file** carries into workflows (INV-GATE-2): parallel writers use `isolation: 'worktree'` and **you own the merge**; shared contracts stay leader-owned.
- **One well-scoped fan-out per workflow.** Read each result and decide the next phase yourself.
- **Guard loops on `budget.total`**, and **`log()` any silent cap** (top-N, sampling) so truncation never reads as full coverage.
- **Self-contained agent prompts**; **no nested orchestration** — a workflow subagent never spawns its own `Workflow`/`Agent` tree; you remain the single integration + verification owner.
- **Set `opts.model` and `opts.effort` explicitly on every non-trivial `agent()`** — they do not reliably inherit.

#### Code staging inside a workflow

Workflow subagents edit isolated worktrees with direct `Edit`/`Write`. The review step is the workflow's own verify stages **plus** your post-hoc diff review of the synthesized output before merging. Comment discipline and the scope invariants bind workflow agents exactly as they bind you.

#### aside / advisor inside a workflow

**Workflow subagents do not call aside or `advisor()`.** (a) cost — N parallel agents each firing paid third-party calls is unbounded quota burn; (b) coherence — a second opinion inside a workflow belongs in its judge/verify stages; (c) the stdio-transport concurrency hazard documented in `claude-agent-kit--aside.md`. You (the leader) own aside/`advisor()` and run them strictly serialized.
