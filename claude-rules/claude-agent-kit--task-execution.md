<!-- claude-agent-kit -->
# Task Execution Protocol

## Before Starting

**File Reading Order:**
1. Project-specific CLAUDE.md (if exists)
2. README files
3. Main implementation files
4. Test files
5. Configuration files

**Pre-check:**
- [ ] Read ALL relevant files completely
- [ ] Identify dependencies and patterns
- [ ] Verify correct path/directory
- [ ] Create plan covering full scope
- [ ] If target files already contain user-owned local changes (check `git status` / `git diff`), read them and plan to preserve — do NOT assume a clean baseline

## Investigation Mode

When asked to investigate, **ONLY investigate** — do NOT make code changes.

**Investigation Template:**
```markdown
## Investigation: [Topic]

### Files Reviewed
- `path/to/file.ts` (123 lines)
- `path/to/other.ts` (456 lines)

### Current Implementation
[Describe what the code actually does]

### Execution Flow
[Trace through the logic]

### Findings
[Bullet points of discoveries]

### Potential Issues
[Any problems identified]
```

## Plan Integrity: Scope Confirmation After Post-Inspection Deferral (HARD RULE)

When a plan explicitly defers scope determination to post-inspection review, the inspection result is a **user-facing checkpoint**, not a license for you to decide scope and continue. Typical phrasings that mark this pattern:

- "Actual scope will be determined after reading the code"
- "Scope TBD pending investigation"
- "We'll decide what to touch once we see how X is structured"
- "코드 확인 후 정한다" / "보고 정하자"
- "Figure out what needs changing and we'll go from there"

The same rule applies when inspection reveals that the approved plan itself reserved scope selection as a post-inspection decision point (the deferral phrasings above are explicit instances; equivalent phrasings also qualify). It does **NOT** apply to *supporting work required to make the approved behavior actually work* — tests, config, imports, minor refactors needed to satisfy the spec are in-scope and proceed without a fresh approval round. *Scope change* means you want to touch files, modules, or behaviors the plan did not name **and** those changes are not required to deliver what was already approved. If uncertain which side a change falls on, ask before acting — but do not paralyze execution on routine supporting work that is clearly required to satisfy the approved deliverable.

**palette note.** palette's loop runs this same checkpoint every cycle: proposing a phase/story scope is a user-facing gate — report the proposed slice, wait for explicit approval, *then* hand off to implementation. The backlog *proposes* scope; the user's approval *authorizes* it. See `claude-agent-kit--palette.md`.

You MUST NOT, after completing inspection:

- **Expand** the plan to cover additional files, modules, or behaviors you discovered, and implement them.
- **Shrink** the plan because inspection showed some parts were unnecessary, and skip them.
- **Substitute** a different approach because you judged it better than the planned one.
- **Continue** to implementation on your own revised scope.

Required sequence:

1. Complete the inspection as planned.
2. **Report findings** — what you found, what scope this implies, what alternatives exist.
3. **Propose a concrete scope** — file list, behaviors, order of operations.
4. **Wait for explicit user approval** of the proposed scope.
5. Only then proceed with implementation.

Clarifications — this rule still applies when:

- **The revised scope looks like the "obvious" or "trivial" next step given what inspection revealed.** The plan deferred the decision precisely so the *user* could make it with the inspection result in hand. Executing your own judgment bypasses that checkpoint, regardless of how self-evident the answer seems.
- **You only want to shrink scope, not expand it.** This is distinct from the `[OVERRIDE]` below that forbids silently reducing a *defined* scope; this rule forbids unilaterally *defining* scope that was deferred to user review. The two are complementary — defined-scope shrinkage is already forbidden, and deferred-scope self-definition is forbidden here. A "small" unilateral definition is still a unilateral definition.
- **You notice an adjacent bug or improvement.** The `CLAUDE.md` Core Principles > Collaboration default says to *mention* adjacent observations — not to *act* on them inside the current task. A deferred-scope plan does not loosen that distinction.

Rationale: the plan's "scope TBD" annotation is a gate, not a waiver. Treating it as a waiver collapses the user's intended decision point into the model's implementation path and discards exactly the review the user asked for.

## Implementation

**Task Documentation (before coding):**
1. **Problem Statement** - Clear description of issue
2. **Root Cause Analysis** - Why is this happening?
3. **Proposed Solutions** - Multiple options with pros/cons
4. **Recommendation** - Which approach and why
5. **Implementation Plan** - Step-by-step breakdown
6. **Risk Assessment** - What could go wrong?

**Task tracking trigger (solo work):** When implementing changes that touch 2+ files or produce 2+ distinct deliverables, call `workslate_task_init` and create tasks BEFORE writing any code. This is the first implementation action. If you realize mid-work that you skipped this, stop and initialize immediately.

**palette note.** In a palette-active project (`_palette/` present), the workslate tasks you create here are the tactical *projection of an already-approved story's acceptance criteria* — a translation, not a second backlog and not new scope. See `claude-agent-kit--palette.md`.

**Preserve user-owned local changes.** Before editing any file, check `git status` / `git diff` for uncommitted changes. Any hunk the model did not make in this session is **user-owned**: do NOT overwrite it, do NOT assume a clean baseline, and do NOT incorporate it into your own edit without explicit authorization. If an edit you are about to make would touch or clobber a user-owned hunk, stop and ask. This applies even when the file itself is "in scope" for the current task — the user's uncommitted work has its own ownership independent of the task's file scope.

**Execution Requirements:**
- Complete task **ENTIRELY** - no partial solutions
- **NO** shortcuts like "... similar for other files"
- Implement ALL necessary changes (files, functions, tests, config)
- Break large tasks into phases, complete each fully
- Track progress with the appropriate task system:

| Context | Task tool | Why |
|---------|-----------|-----|
| Solo work | `workslate_task_*` | Footer auto-display, named sessions, disk persistence |
| Team leader | `workslate_task_*` (`ws:` own phases, `team:` task graph) | Unified tracking — footer shows both namespaces |
| Teammate | `workslate_task_*` (`team:` namespace) | Same SQLite DB, concurrent via WAL, self-claim via `workslate_task_update(owner=self)` |

**[OVERRIDE]** `"Don't add features, refactor, or introduce abstractions beyond what the task requires."` / `"Don't design for hypothetical future requirements."` / `"Three similar lines is better than a premature abstraction."`

In this project: when a design document or implementation plan is provided, implement the **entire specified scope**. Do not shrink it. Do not substitute a "simpler approach." Do not produce stubs, placeholders, TODOs, or "for now" implementations. Do not defer any part of the specified scope to a follow-up PR, a subsequent commit, a "next round," or a future ticket — this is scope reduction even when announced openly. See **Core Principles > Quality Standards** in the main `CLAUDE.md` for the full rule (it also applies to prose requests, not just design docs, and closes the `silently reduce scope` loophole). The design document IS the specification — follow it completely. If you believe part of the spec is wrong, genuinely impossible, that you must reorder operations to prevent a regression, or that you need a design that deviates from the approved plan, do **not** act on that judgment — stop and re-request explicit approval per **Forced Spec/Plan Deviation: Re-request Approval** below. Do not silently or openly reduce scope.

The system-prompt directive above governs *unsolicited* expansion — don't refactor or introduce abstractions the user didn't ask for. It does NOT authorize *contracting* the asked-for scope. Those are different axes.

**palette note.** palette's thin-slicing does not license contracting an approved scope: choosing a *thinner slice* is a planning-time, user-owned scope decision (which this project already mandates) made *before* approval — not the agent shrinking work already approved. Once a slice is approved, "implement the entire scope" applies to that slice, and deferring any of it to the backlog needs explicit, specific user consent. See `claude-agent-kit--palette.md`.

**[OVERRIDE]** `"Prefer editing existing files to creating new ones."`

If the design document or task specifies creating new files, create them — the design document or task description constitutes the explicit case this preference isn't meant to block. This is specifically meant to suppress the agent-side failure mode of responding to "split `main.rs` into modules per the plan" by editing `main.rs` in place to avoid creating the new module files.

**Refactoring Guidelines:**

When to refactor:
- Code duplicated 3+ times
- Function does too many things (>50 lines)
- Clear naming/structure improvement

When NOT to refactor:
- Without test coverage
- Mid-feature (complete feature first)
- When it would touch unrelated code

**Comment Discipline:**
- Write comments only when the **WHY** is non-obvious. Do not explain WHAT code does — the code itself should be readable.
- Do not remove existing comments unless you are removing the code they describe.
- No boilerplate comments, no restating the function signature in prose.
- **No chain-of-thought in output.** Never write your reasoning process — self-corrections ("Actually:", "Correction:"), step-by-step deliberation, working through alternatives, or false starts — into code comments, commit messages, or conversation text. Resolve your thinking internally. Only the final, correct conclusion belongs in output. If reasoning is complex enough to need documentation, write a concise explanation of the conclusion, not the journey to it.

## Forced Spec/Plan Deviation: Re-request Approval (HARD RULE)

Scope is **NEVER** reduced arbitrarily, and the agent **NEVER** decides a deviation on the user's behalf. This section is a *deviation gate*, not permission to reduce scope: when one of the three triggers below fires, you may only **pause and ask** — you may not implement the deviation, a reduction, or a preferred alternative on your own judgment.

**palette note.** For a palette story, its `Done when` + `Not this story` **are** the "approved spec/plan" this gate protects. A forced deviation while implementing a story follows the same stop → preserve → propose → wait procedure; moving unmet acceptance criteria into the backlog is not an escape hatch. See `claude-agent-kit--palette.md`.

The ONLY three situations that justify deviating from the approved spec/plan — **each REQUIRES you to stop and re-request EXPLICIT user approval before proceeding**:

1. **Genuinely impossible as specified.** The approved spec cannot be satisfied under the actual constraints of the repository, platform, APIs, permissions, or logic, *even with reasonable implementation effort*. "Impossible" does NOT mean expensive, tedious, unfamiliar, risky, time-consuming, hard to test, aesthetically undesirable, or inconvenient.
   - *Disambiguating tests:* if the original behavior can still be delivered by writing more code, adding required tests/config, or doing the tedious part → it is NOT impossible. If the objection is "too much work" / "messier than expected" / "I prefer another design" → NOT impossible. You must cite a **concrete blocking fact**: missing API capability, mutually contradictory requirements, unavailable permission, invariant conflict, platform limitation, or a failing proof-of-concept. If only part is impossible, identify the **smallest impossible sub-requirement** — do not relabel the whole scope impossible.

2. **Reordering to prevent an ordering-induced regression.** The approved order of operations would itself cause a regression, so operations must be reordered to decouple it. Re-approval is required **only** when the reorder changes the approved delivery sequence, what is delivered or when, integration boundaries / risk, migration safety, test expectations, or user-visible behavior — or when the approved order was itself part of the plan. **Purely internal coding order that preserves the approved deliverable and behavior is ordinary execution and needs no approval.**

3. **A design decision that deviates from the approved plan.** A change to an approved architectural, behavioral, data-model, file-boundary, dependency, API, persistence, concurrency, security, migration, or testing decision. This is **NOT** a license to replace the approved plan with a cleaner, smaller, or preferred design.
   - *Disambiguating tests:* if a reviewer comparing the approved plan to the implementation would say "this is a different approach" (even with similar behavior) → re-approval. If the change is because the original plan is wrong / unsafe / incompatible with discovered facts → re-approval. If it is local implementation judgment *inside* the approved design with no behavioral / scope / interface / deliverable change → no re-approval.

**Anything outside these three follows the existing no-reduction rule** (see the **Implementation** `[OVERRIDE]` above and `CLAUDE.md` Core Principles > Quality Standards).

**Anti-loophole — "I discovered it mid-way" is not a free pass.** The valid trigger is *new concrete information that was not reasonably knowable before implementation / verification* — not the mere fact that you are mid-work. If the blocking fact was reasonably discoverable during the required pre-implementation reading / planning phase (see **Before Starting** > **Pre-check**), admit the planning miss, stop, and ask; do not present a foreseeable issue as a mid-implementation surprise. Foreseeable "this scope is too large" concerns are still raised **before starting** (per `CLAUDE.md` Quality Standards), not at completion time.

**Required action when a trigger fires:**
1. **Preserve all work-so-far** — no rollback, deletion, blanking, or hiding of incomplete state (the same prohibition as **Undo / Revert Handling** subsection A below; deviation pressure and rollback pressure are adjacent failure modes).
2. State the **approved requirement** at issue.
3. State the **concrete discovered fact** that forces the deviation.
4. Explain **why** the existing scope cannot / should not proceed unchanged.
5. **Enumerate what you can still deliver** under the original plan, isolating the single deviation point.
6. **Propose the smallest concrete deviation** and describe its behavioral / file / test / delivery impact.
7. **Wait for explicit user approval** before continuing past the deviation point.

This gate is the mirror of the scope-reduction prohibition it sits beside: the prohibition stops you from *quietly shrinking* the work; this gate stops you from *unilaterally changing* it. Both resolve the same way — surface it, propose, wait.

## Undo / Revert Handling (HARD RULE)

In a Claude Code session, "revert" / "undo" / "discard" / "roll back" / "되돌려" and equivalents refer by default to **reversing the edits the model made in this session** — not to running git operations. This section governs both how you respond to such user requests (subsection B) and when you are allowed to unwind your own work at all (subsection A). A narrow carve-out (subsection C) applies only when the user *explicitly names* a git command.

### A. Model-initiated rollback is forbidden

If you judge mid- or post-implementation that the scope is too large, that your approach was wrong, or that the work so far should be thrown away, you MUST NOT use any mechanism to undo, destroy, or hide the work. Forbidden mechanisms include (non-exhaustive — the list extends to any tool whose effect is to erase the incomplete state):

- **Destructive git operations.** `git checkout --` / `git restore` / `git reset --hard` / `git revert` / `git clean -f*` / `git stash drop` / `git branch -D` / `git push --force*`, and any equivalent.
- **`Edit` / `Write` used to overwrite, blank out, or replace your own work.** Using `Edit` with an empty `new_string`, or `Write` with cleared content, to erase code you just wrote is the same failure mode as `git checkout --`, just through a different tool surface.
- **File or directory deletion** — `rm`, Bash-level deletes, or deleting new files you created earlier in the session.
- **Any shell command, MCP tool, or action whose purpose is to erase the incomplete state**, regardless of tool surface.

Required procedure when the trigger fires:

1. **Stop.** Do not run any of the mechanisms above.
2. **Preserve state.** Files, commits, stashes, and branches stay exactly as they are.
3. **Report to the user.** Cover (a) what was completed, (b) what remains, (c) why you believe the current direction is wrong or the scope cannot be finished, (d) the current state of files and repo.
4. **Wait for direction.** The user decides whether to roll back, split the work, change approach, or keep partial work. Rollback-direction choice is a user decision with consequences you do not own.

**Distinct from normal iteration.** Fixing a bug you introduced earlier in the session, refactoring code you just wrote, or correcting typos inside the same approved scope is NOT rollback — it is normal forward development and is fine. Rollback is when you judge the *direction itself* was wrong and want to erase the work to start over or give up; that requires user direction, not self-judgment.

**Observable test for the trigger.** If the net effect of the action you are about to take is to *remove* or *blank out* code / files you created earlier in this session **without replacing them with the approved deliverable**, that action is rollback — regardless of how you label it internally ("cleanup", "simplification", "refactor", "try a different approach"). Forward iteration always moves toward the approved deliverable; rollback moves away from it. Use this test to catch intent-mislabeling in yourself.

**Adjacent failure mode — deviation pressure.** When the impulse is not "erase the work" but "the scope / approach / order must *change*" (it is impossible as specified, the planned order would cause a regression, or the design must deviate from the plan), that is governed by **Forced Spec/Plan Deviation: Re-request Approval** above — same resolution: stop, preserve state, propose, wait for explicit approval; never act unilaterally.

### B. User-requested revert / undo: reverse session edits via file edits

When the user says "revert", "undo", "discard these changes", "roll this back", "되돌려", or anything equivalent in the context of work done during this session, the default interpretation is:

**Reverse the session's file edits by editing the files back to their pre-edit state — not by running any git operation.**

Why: the edits made in the session are edits. They live in the files on disk. Undoing them is also an edit — write the inverse content. Git operations touch *repo state*, which includes the user's out-of-session work (uncommitted changes in files the model never edited, unrelated commits, stashes, branches) that you have no view into. Reaching for git to undo a session edit is a category error whose failure mode is collateral destruction of work the user never asked you to touch.

Required procedure:

1. **Identify what edits the model made in this session.** Sources, in order of reliability: the `Edit` / `Write` tool uses visible in the conversation history; the conversation's narration of what was changed.
2. **Confirm reconstructibility.** If you cannot reconstruct the pre-edit content with high confidence — long session with compacted history, or changes whose exact prior content the conversation did not preserve — do NOT perform an approximate undo. Report exactly which parts you are and are not confident about, and ask the user whether to inspect `git diff` / file history or to name an explicit git command.
3. **Confirm scope with the user.** Which edits specifically — all of them, just the most recent, a specific file, a specific hunk? If the user's phrasing is ambiguous, ask before touching anything.
4. **Reverse the edits via `Edit` / `Write`.** Write the inverse operation: delete the lines you added, restore the lines you replaced, remove the files you created in this session.
5. **Do NOT reach for git for session-edit undo.** Not `checkout --`, not `restore`, not `revert`, not `reset`, not `stash`, not any other git command. None of those are the right tool for undoing session edits. (See step 6 for the case where the user's request is actually about a commit / branch / ref, not session edits.)
6. **If the user identifies a commit / branch / ref** (e.g., *"revert commit abc123"*, *"undo what's on main since yesterday"*, *"remove the commit you just made"*), stop and clarify which git operation they want — this is NOT session-edit undo regardless of whether the commit came from this session. Do not reinterpret it as file-edit undo. Subsection C applies once the user names a specific git command.
7. **If the undo would require touching files the model did NOT edit in this session**, stop and clarify. Those files' state is user-owned, not session-owned; you need explicit authorization before changing them.

### C. Explicit git-command requests (narrow carve-out, HARD RULE)

A destructive git operation may be run ONLY when the user **explicitly names the git command** in their request — e.g., *"run `git reset --hard HEAD~1`"*, *"do `git checkout -- foo.ts`"*, *"use `git revert abc123`"*. Generic phrasings like "revert it", "undo that", "throw that away", "roll back", "되돌려" do NOT name a git command and fall under subsection B — do not translate them into git operations on your own.

When a git command is explicitly named, apply this pre-flight before running it:

1. **Identify the named command exactly.** Same command, same arguments, no substitution.
2. **Inspect surrounding state.** Run `git status` and `git stash list` for working-tree state. **For commands that affect commit history** (`reset`, `rebase`, `revert`, `cherry-pick`, `branch -D`, `push --force*`), also run `git log --oneline` / `git log --graph` / `git reflog` as needed to enumerate every commit / branch / ref the candidate command would change. `git status` alone is insufficient for commit-graph-affecting commands.
3. **Propose the command with its full blast radius**, including:
   - the exact command line,
   - every file / commit / stash / branch it would change (not just what the user named),
   - whether state is preserved or destroyed,
   - any risks (merge conflicts, data loss, unreferenced objects).
4. **Wait for explicit per-command authorization.** A "yes, run it" / "go ahead" against a specific proposed command counts; a generic "just run it" against an ambiguous earlier phrasing does not — re-propose until there is a specific authorized command.
5. **Execute only the authorized command.** Do NOT substitute a different command even if it seems equivalent or safer. If the proposed command's blast radius worries you, say so in the proposal — but do not unilaterally switch commands.

**Project-mandatory flags.** If project rules (see `claude-agent-kit--git-workflow.md` > Commit Rules) require specific flags on the named command — for example the standing `--no-gpg-sign` requirement on `git commit` / `git commit --amend` / `git revert` / `git cherry-pick` — surface the modified command in your proposal (e.g., propose `git revert HEAD --no-gpg-sign`, not `git revert HEAD`) and get explicit authorization for the actual invocation. Silently appending mandatory flags to the user's exact phrasing is substitution; *surfacing* them in the proposal is not.

If the surgical option the user named does not exist, or if the user's named command would destroy more than they seem to intend, stop and describe exactly what else will be affected. Wait for the user to either authorize the broader blast radius explicitly or supply an alternative command.

The user owns **what** to undo, **which specific command** runs, and **when** it runs. The model's role is to surface the option space and the blast radius of each candidate — not to choose or execute on the user's behalf.

## Task Sessions

**`workslate_task_init(name)` is mandatory before using any task tool.** Tasks are stored in SQLite (`workslate.db`) and shared across all agent instances in the same project. The project standardizes on `workslate_task_*` for both solo and team tracking — only it is surfaced by the doorbell footer on every tool call and shared cross-session via the DB. The built-in `TaskCreate` / `TaskList` / `TaskUpdate` tools also exist under the single implicit team (with native self-claiming), but `workslate_task_*` is the system of record here — do not split coordination across both.

**Namespaces:** Tasks use `ws:` (personal) or `team:` (team coordination) prefixes:
- `workslate_task_create("Fix auth", namespace="ws")` → creates `ws:1`
- `workslate_task_create("Port handlers", namespace="team", owner="backend-dev")` → creates `team:1`
- `workslate_task_done("team:1")` — ID format: `"3"` (defaults to ws), `"ws:3"`, or `"team:3"`

**Cross-namespace dependencies:** `depends_on: ["ws:1", "team:2"]` — a task can depend on tasks in either namespace.

**Footer** shows both namespaces: `── Tasks (session) ws:[3/5] team:[8/12] ──`

**Workflow:**
1. `workslate_task_init("auth-refactor")` — create or resume a named session
2. `workslate_task_create(name, namespace?, owner?, depends_on?)` — create tasks
3. `workslate_task_done("ws:1")` / `workslate_task_update("team:3", status="in_progress")` — update
4. `workslate_task_list(namespace?)` — list tasks, optional namespace filter
5. `workslate_task_sessions()` — list all sessions with per-namespace counters

**Rules:**
- `workslate_task_init` must be called before any task operation
- Only one session is active at a time per MCP server instance
- Switching sessions does NOT clear the previous session's tasks (SQLite persists)
- Restarting the MCP server clears the active session — call `workslate_task_init` again to resume
- Multiple agent instances can read/write the same session concurrently (SQLite WAL mode)

## Anti-self-grading Stop verify hook

`make install` registers (alongside the doorbell hooks) a `type:"agent"` Stop hook that spawns an independent verifier subagent before Claude is allowed to end a turn. The verifier reads the turn's transcript tail for completion claims (tasks finished, tests passing, builds green), spot-checks the load-bearing claims against **real repository state** (reads the files, runs the cited tests/commands), and returns `{"ok": true}` to allow the stop or `{"ok": false, "reason": ...}` to block it — the reason is fed back to Claude as its next instruction. A turn with no completion claims passes immediately, and `stop_hook_active` bounds the block to one retry per turn.

**It is deliberately standalone.** The verifier has no workslate dependency — it judges the conversation's claims against the repo directly. Task tracking (`workslate_task_*`) and stop-verification are orthogonal: the board records intent and progress; the hook audits completion claims.

**Relationship to `/goal`.** Claude Code's native `/goal` command (v2.1.139+) is a *session-scoped, user-typed* Stop hook whose tool-less evaluator judges transcript text alone. This hook is complementary: installed once, it applies to every session, and its verifier has tool access (reads files, runs tests) rather than judging text alone. Both can be active and fire independently on Stop.

**Cost tradeoff, accepted by design.** Claude Code's hook system has no mechanism for one hook entry to gate whether a sibling entry even fires, so this `type:"agent"` entry spawns a verifier subagent on *every* Stop event, in *every* session — a real cost/latency addition on top of the otherwise low-cost, `command`-type doorbell hooks. Bundled into the default install anyway (rather than a separate opt-in) since this is a single-user personal toolchain, not a distribution weighing many installs' worth of that cost. Uninstall with `workslate --uninstall-hooks` (removes all workslate-owned hooks, including this one) if the tradeoff stops being worth it for a given project.

## Team Messaging Tools (Agent Teams)

For multi-agent coordination, workslate exposes `workslate_register(role, session_id, agent_id)`, `workslate_msg_send(recipient, subject, body, urgent?, session_id?, agent_id?)`, and `workslate_inbox_read(role)`, enabling **mid-turn steering** of running teammates via per-tool-call doorbell hooks. The full mechanics — leader/`team-lead` identity, the `session_id`+`agent_id` requirement on every `msg_send`, startup sequence, role addressing — are canonical in `claude-agent-kit--parallel-work.md` → **Mid-Turn Steering & Team Messaging**; this is a pointer, not a restatement. Note: task status is now surfaced by the doorbell hook on every tool call (installed by `make install`), not appended to workslate tool results.

## After Completion

- [ ] All deliverables complete
- [ ] No placeholders or TODOs remain
- [ ] Tests pass (if applicable) — **actually verified, not assumed**
- [ ] No regression in related features
- [ ] Linting/type checking passes (if applicable)
- [ ] Outcome reported faithfully — failures disclosed, not hidden
