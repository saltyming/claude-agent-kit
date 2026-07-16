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

Three mechanisms parallelize work in Claude Code — two via the `Agent` tool, one via the separate **`Workflow`** tool. The fourth surface, **`dispatch`**, is external execution delegation (shared taxonomy above; `claude-agent-kit--dispatch.md`).

- **Subagents** — `Agent` fire-and-forget: runs once, returns a result. Read-only types (`Explore`, `Plan`) are free and proactive; `general-purpose` is the write-capable type (GATE-DELEGATE applies) for genuinely **independent** subtasks.
- **Teammates** — `Agent` with `name` + `run_in_background: true`: a persistent, steerable peer in the single implicit team (no `TeamCreate`; `team_name` is ignored). For **dependent, coordinated** work — 3+ parallel streams that must talk or be steered mid-task. A foreground or unnamed `Agent` call is a subagent, not a teammate.
- **`Workflow`** — a separate tool running a deterministic script over many subagents (fan-out/pipeline), current-turn opt-in only (see *Workflow* below). `Agent` exposes `model` but no effort knob; Workflow's `agent()` exposes both — set them explicitly, they don't inherit.

### Teams — cost and composition

Each teammate is a full Claude Code instance (loads CLAUDE.md, MCP, skills; does NOT inherit your conversation); a 5-teammate team costs roughly 3–5× solo. Match the mechanism to the work: <5 files → single session, even if asked (explain why); 5–10 files → single session, or 1–2 subagents for isolated edits; 10+ files with clean non-overlapping scopes → a team is justified; overlapping scopes / shared types dominate → still single session. Scale informs what you *propose*, never whether to spawn unprompted (GATE-DELEGATE). Cap teams at 3–5 teammates; default them to `model="sonnet"` (escalate one to Opus only for demonstrated cross-module reasoning load); the leader stays on the session model and owns integration. Implementation teammates need a write-capable `subagent_type` — never `Explore`/`Plan`. Creation prompts carry **role + file scope only, never tasks** (a teammate with tasks in its prompt starts before the task graph exists).

### Coordination — built-in task list + workslate messaging

**Tasks:** the built-in task list (`TaskCreate` / `TaskUpdate` / `TaskList`) is the team's coordination system — the leader designs the task graph, reserves shared types / integration / cross-scope work for itself, and teammates self-claim eligible tasks (unblocked, unassigned, inside their file scope, not touching shared contracts). One writer per file (INV-GATE-2); teammate scopes must not overlap.

**Messaging:** native `SendMessage` is the channel — but it delivers only at the recipient's turn boundary. The workslate bridge closes the gap: a PostToolUse hook mirrors every native send into a SQLite inbox, and the recipient's PreToolUse doorbell announces unread messages **mid-turn**. Wiring is near-zero-touch: teammates are auto-registered under their agent name by the SubagentStart hook (the hint says so; if it instructs manual registration, call `workslate_register` with the hint's BOTH ids); the **leader registers once** — `workslate_register(role="team-lead", session_id=<SessionStart hint value>, agent_id="")` — so teammate→main traffic reaches its doorbell too. When the doorbell reports unread, drain with `workslate_inbox_read(role=<yours>, session_id=<hint value>)`; a bridged message read early will also arrive natively at the next turn boundary — a duplicate is expected, not a re-send. Call `workslate_msg_send` directly only for `urgent=true` steering that must interrupt. Do not use messages to coordinate task dependencies — that is the task list's job.

### Leader workflow

1. Register as team-lead (above) and drain your inbox once.
2. Spawn teammates (`Agent`, named, backgrounded, sonnet, role+scope prompt) — they explore their scope while waiting.
3. Design the task graph with `TaskCreate`; reserve shared contracts for yourself.
4. Monitor; intervene only on: inconsistent assumptions across reports, a silent stall on a claimed task, downstream failure after an upstream "completion", scope drift, duplicated work. **A teammate that stopped without your shutdown, normal completion, or an error report = assume the user interrupted it directly — hold its work, surface "waiting for user direction", do not re-assign or replace.**
5. Build & verify after completions; fix integration (imports, visibility, wiring) yourself.
6. Shut down every teammate (`shutdown_request`) when done.

### Teammate behavior

On creation: explore your scope only — do not implement; wait for tasks. Claim → work within scope → report → claim next. Report blockers to the leader immediately; never run build/test yourself (ask the leader); never shrink a task's scope (INV-GATE-3); message dependents directly when your output (types, APIs, formats) feeds their tasks. Before idling, drain your inbox (the doorbell only fires while you run tools).

**Completion report (HARD RULE)** — plain text to the leader, under ~500 tokens: `TASK: <id> — DONE` / `CHANGED:` file:line-range + 1-line each / `VERIFICATION:` concrete evidence ("grep 'fn old_name' → 0 matches"), not assertions / optional `DEFERRED:` + `GOTCHA:` (one-line trap worth propagating) / `NEXT:` ready-for-X | blocked-on-Y. No narration, no pasted code.

### Anti-patterns

6+ teammates (overhead dominates — cap 5); leader hand-dispatching every task (self-claim exists); leader skipping the build (integration issues found late); task instructions in creation prompts; `Explore`/`Plan` teammates for implementation (cannot edit — silent failure); messaging for dependency coordination; a Workflow `agent()` left on default model/effort. No session resume for in-process teammates (`/resume` won't restore them); task status can lag — check and update manually when stuck.

### Workflow (the third delegation surface)

`Workflow` runs a deterministic JS script orchestrating many subagents — for large, breadth-first, mechanical work (codebase-wide sweeps, N-file migrations, reviewer panels). **Real capability, never the default**: treat like any write-capable delegation — surface/propose → run on agreement. Valid opt-ins: `ultracode` confirmed for the current turn by a system-reminder; the user asking for a workflow in their own words; a user-invoked skill whose instructions call it; or the user agreeing to one you proposed. Never fire off stale or inferred opt-in. `ultracode` raises thoroughness (author workflows for substantive tasks, multi-phase with you in the loop) — it does NOT collapse the approval gate, license scope reduction, or replace your own verification of the synthesized result; budget exhaustion is not completion (stop, report remaining scope, ask).

Quality flow: pipeline by default, barrier only when a stage genuinely needs all prior results; diversify verifier lenses; one writer per target file (parallel writers → `isolation: 'worktree'`, you own the merge); one well-scoped fan-out per workflow — read each result and decide the next phase yourself; guard loops on `budget.total` and `log()` any silent cap; self-contained agent prompts; no nested orchestration; set `opts.model`/`opts.effort` explicitly on every non-trivial `agent()`. **Workflow subagents never call aside or `advisor()`** — cost, coherence, and the stdio-concurrency hazard; you own those surfaces, strictly serialized.
