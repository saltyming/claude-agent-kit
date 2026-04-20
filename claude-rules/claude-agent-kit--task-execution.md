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

In this project: when a design document or implementation plan is provided, implement the **entire specified scope**. Do not shrink it. Do not substitute a "simpler approach." Do not produce stubs, placeholders, TODOs, or "for now" implementations. Do not defer any part of the specified scope to a follow-up PR, a subsequent commit, a "next round," or a future ticket — this is scope reduction even when announced openly. See **Core Principles > Quality Standards** in the main `CLAUDE.md` for the full rule (it also applies to prose requests, not just design docs, and closes the `silently reduce scope` loophole). The design document IS the specification — follow it completely. If you believe part of the spec is wrong, say so explicitly and wait for a decision. Do not silently or openly reduce scope.

The system-prompt directive above governs *unsolicited* expansion — don't refactor or introduce abstractions the user didn't ask for. It does NOT authorize *contracting* the asked-for scope. Those are different axes.

**[OVERRIDE]** `"ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required."`

If the design document or task specifies creating new files, create them — the design document or task description constitutes the "explicit requirement" the system prompt asks for. This is specifically meant to suppress the agent-side failure mode of responding to "split `main.rs` into modules per the plan" by editing `main.rs` in place to avoid creating the new module files.

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
- **No chain-of-thought in output.** Never write your reasoning process — self-corrections ("Actually:", "Correction:"), step-by-step deliberation, working through alternatives, or false starts — into code comments, commit messages, conversation text, or workslate buffers. Resolve your thinking internally. Only the final, correct conclusion belongs in output. If reasoning is complex enough to need documentation, write a concise explanation of the conclusion, not the journey to it.

## Undo / Revert Handling (HARD RULE)

In a Claude Code session, "revert" / "undo" / "discard" / "roll back" / "되돌려" and equivalents refer by default to **reversing the edits the model made in this session** — not to running git operations. This section governs both how you respond to such user requests (subsection B) and when you are allowed to unwind your own work at all (subsection A). A narrow carve-out (subsection C) applies only when the user *explicitly names* a git command.

### A. Model-initiated rollback is forbidden

If you judge mid- or post-implementation that the scope is too large, that your approach was wrong, or that the work so far should be thrown away, you MUST NOT use any mechanism to undo, destroy, or hide the work. Forbidden mechanisms include (non-exhaustive — the list extends to any tool whose effect is to erase the incomplete state):

- **Destructive git operations.** `git checkout --` / `git restore` / `git reset --hard` / `git revert` / `git clean -f*` / `git stash drop` / `git branch -D` / `git push --force*`, and any equivalent.
- **`Edit` / `Write` / `workslate_apply` used to overwrite, blank out, or replace your own work.** Using `Edit` with an empty `new_string`, or `Write` / `workslate_write` with a cleared buffer, to erase code you just wrote is the same failure mode as `git checkout --`, just through a different tool surface.
- **File or directory deletion** — `rm`, Bash-level deletes, or deleting new files you created earlier in the session.
- **Any shell command, MCP tool, or action whose purpose is to erase the incomplete state**, regardless of tool surface.

Required procedure when the trigger fires:

1. **Stop.** Do not run any of the mechanisms above.
2. **Preserve state.** Files, buffers, commits, stashes, and branches stay exactly as they are.
3. **Report to the user.** Cover (a) what was completed, (b) what remains, (c) why you believe the current direction is wrong or the scope cannot be finished, (d) the current state of files and repo.
4. **Wait for direction.** The user decides whether to roll back, split the work, change approach, or keep partial work. Rollback-direction choice is a user decision with consequences you do not own.

**Distinct from normal iteration.** Fixing a bug you introduced earlier in the session, refactoring code you just wrote, or correcting typos inside the same approved scope is NOT rollback — it is normal forward development and is fine. Rollback is when you judge the *direction itself* was wrong and want to erase the work to start over or give up; that requires user direction, not self-judgment.

**Observable test for the trigger.** If the net effect of the action you are about to take is to *remove* or *blank out* code / files you created earlier in this session **without replacing them with the approved deliverable**, that action is rollback — regardless of how you label it internally ("cleanup", "simplification", "refactor", "try a different approach"). Forward iteration always moves toward the approved deliverable; rollback moves away from it. Use this test to catch intent-mislabeling in yourself.

### B. User-requested revert / undo: reverse session edits via file edits

When the user says "revert", "undo", "discard these changes", "roll this back", "되돌려", or anything equivalent in the context of work done during this session, the default interpretation is:

**Reverse the session's file edits by editing the files back to their pre-edit state — not by running any git operation.**

Why: the edits made in the session are edits. They live in the files on disk. Undoing them is also an edit — write the inverse content. Git operations touch *repo state*, which includes the user's out-of-session work (uncommitted changes in files the model never edited, unrelated commits, stashes, branches) that you have no view into. Reaching for git to undo a session edit is a category error whose failure mode is collateral destruction of work the user never asked you to touch.

Required procedure:

1. **Identify what edits the model made in this session.** Sources, in order of reliability: the `Edit` / `Write` / `workslate_apply` tool uses visible in the conversation history; workslate buffer / task records (`workslate_task_sessions`, `workslate_diff` against disk); the conversation's narration of what was changed.
2. **Confirm reconstructibility.** If you cannot reconstruct the pre-edit content with high confidence — long session with compacted history, direct `Edit` calls not captured in workslate, auto-cleared workslate buffers, or changes whose exact prior content the conversation did not preserve — do NOT perform an approximate undo. Report exactly which parts you are and are not confident about, and ask the user whether to inspect `git diff` / file history or to name an explicit git command.
3. **Confirm scope with the user.** Which edits specifically — all of them, just the most recent, a specific file, a specific hunk? If the user's phrasing is ambiguous, ask before touching anything.
4. **Reverse the edits via `Edit` / `Write` / `workslate_edit` / `workslate_apply`.** Write the inverse operation: delete the lines you added, restore the lines you replaced, remove the files you created in this session.
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

## Code Staging

**Non-trivial or multi-hunk code changes go through workslate first.** Trivial single-block edits may use direct `Edit` per the exceptions below. For everything else, the review step before application catches chain-of-thought leaking into comments and unintentional scope reduction, both of which occur frequently with direct edits. **Never call `workslate_apply` without first reviewing the diff** — the diff step is the entire point.

Three staging modes exist — all return the diff for review:

| Tool | Use case |
|------|----------|
| `workslate_edit(name, file_path, old, new, position?, match_index?, line_start?, line_end?)` | Load file from disk + edit (creates/overwrites buffer) |
| `workslate_edit(name, old, new)` | Edit existing buffer content (no file_path = buffer mode) |
| `workslate_write(name, content, file_path, depends_on?)` | Full file creation/rewrite (new files show full content with line numbers) |

`file_path` is the disambiguator: present = load from disk, absent = edit buffer.

**One buffer per file.** The server enforces this: creating a second buffer targeting the same file returns an error. Use a single buffer and chain edits, or clear the old buffer first. Buffers persist in SQLite and survive server restarts.

Two read tools support the staging workflow:

| Tool | Use case |
|------|----------|
| `workslate_read(file_path)` | Read a file from disk with line numbers — use to get precise line coordinates before editing |
| `workslate_search(file_path, pattern, regex?, context?)` | Find patterns and return matches with line numbers. Plain substring by default; use `regex=true` for regex (e.g. `FOO\|BAR`) |

**Typical precision-edit workflow:**
1. `workslate_search(file_path, "fn target_function")` — find the function, get line numbers from Summary
2. `workslate_read(file_path, start_line, end_line)` — read the exact range with line numbers to confirm
3. `workslate_edit(name, file_path, line_start, line_end, new_string)` — edit by line range, review diff
4. `workslate_apply(name)` — apply

**Large file editing pattern (buffer-first):**
1. `workslate_edit(name, file_path, old_string, new_string)` — load file + first edit
2. `workslate_edit(name, old_string, new_string)` — subsequent edits on stable buffer (no line drift)
3. `workslate_diff(name, summary=true)` — quick check: "2 hunks, +15/-8 lines"
4. `workslate_diff(name)` — full diff for review if needed
5. `workslate_apply(name)` — apply

This avoids the line-shifting problem: once loaded into the buffer, edits operate on stable content regardless of external file changes.

`position` values for `workslate_edit`:
- omitted or `"replace"` — find old_string, replace with new_string (default)
- `"after"` — find old_string as anchor, insert new_string after it (anchor stays)
- `"before"` — find old_string as anchor, insert new_string before it (anchor stays)
- `"append"` — append new_string to end of file (old_string not needed)

Targeting options (apply to all position modes except append):
- **Default** — old_string must appear exactly once in the file
- `match_index: N` — target the Nth occurrence of old_string (1-based). Use when old_string isn't unique.
- `line_start: N` (+ optional `line_end: M`) — target by line range instead of old_string. 1-based, inclusive. old_string is not needed.

**When to use Edit directly (exceptions):**
- Small single contiguous change (single-block replacement, NOT a full-file rewrite — rewrites must still go through workslate)
- Import additions/removals
- String/message literal updates
- Renaming (use `replace_all`)

**When workslate is mandatory (no exceptions):**
- Editing 2+ non-adjacent sections of the same file
- Inserting code between existing code (`position: "after"` / `"before"`)
- Appending to a file (`position: "append"`)
- Any file creation with more than trivial content

**Partial replacement workflow (existing file):**
1. `workslate_edit(name, file_path, old_string, new_string)` — load file from disk, apply edit, review diff
2. If more edits needed: `workslate_edit(name, old_string, new_string)` — edits buffer (no file_path = chains with previous)
3. `workslate_apply(name)` — uses stored file_path; buffer auto-clears on success

**Full file workflow (new file):**
1. `workslate_write(name, content, file_path)` — draft the full content, review the returned diff
2. If issues found: `workslate_edit(name, old_string, new_string)` — edits buffer directly (no file_path = buffer mode)
3. `workslate_apply(name)` — uses stored file_path; buffer auto-clears on success

`workslate_clear` is only needed to abandon a buffer without applying it. Successful `workslate_apply` removes the buffer from both memory and SQLite automatically.

`workslate_diff(name)` remains available for re-checking a buffer against its target file at any time. Use `workslate_diff(name, summary=true)` for a one-line stat ("N hunks, +X/-Y lines") to save context.

**Buffer dependencies:** `workslate_write(name, content, file_path, depends_on=["buf-a", "buf-b"])` declares that this buffer must be applied after buf-a and buf-b. `workslate_apply` enforces the ordering.

**Dry run:** `workslate_apply(name, dry_run=true)` shows the final file content with line numbers without writing to disk.

**Parameter types (HARD RULE).** Workslate MCP fields take **native JSON types** — pass JSON arrays, booleans, and numbers, never JSON-encoded strings:

```
depends_on: ["ws:1", "team:2"]   ✓ JSON array
depends_on: "[\"ws:1\"]"          ✗ stringified array — don't

dry_run: true                     ✓ JSON boolean
dry_run: "true"                   ✗ string

match_index: 2                    ✓ JSON integer
match_index: "2"                  ✗ string
```

The server tolerates the stringified forms as a best-effort shim, but treat this as a bug in your tool call — aim to send raw JSON values every time. Applies to every array/bool/int field across `workslate_task_create`, `workslate_write`, `workslate_edit`, `workslate_read`, `workslate_search`, `workslate_diff`, `workslate_apply`, `workslate_clear`.

**Rules:**
- **Always pass `file_path` to `workslate_write`** so the diff is returned for review. Omitting it skips the review — only acceptable for scratch buffers not destined for files.
- Use descriptive buffer names that indicate the target (e.g., `auth-middleware`, `lock-ordering-fix`)
- Chain-of-thought prohibition applies equally to staged code — no reasoning in comments
- `workslate_apply` auto-clears the buffer on success; only call `workslate_clear(name=...)` to **abandon** a buffer you no longer want to apply
- When working in Agent Teams, each teammate should use buffer names prefixed with their scope to avoid collisions

### Workslate safety rules (HARD RULES)

These rules prevent catastrophic loss of staged work. The code enforces the first rule; the others are behavioral.

- **`workslate_clear()` without arguments is forbidden.** The tool now requires either `name="<buffer>"` or `all=true` explicitly. This exists because a bare call in Agent Team scenarios can wipe every teammate's staged work in one step. If you want to clear everything, pass `all=true` and you will see the list of buffers being cleared — use that as a last checkpoint.
- **Buffer names must be prefixed with context** so multiple agents in the same project (solo sessions, team leader, teammates) do not collide on a shared key:
  - Solo work: `<module>-<file>` — e.g., `vfs-main-rs`, `auth-middleware`
  - Team leader: `leader-<file>` — e.g., `leader-types-rs`
  - Teammate: `<teammate-name>-<file>` — e.g., `posix-libs-at-rs`, `backend-api-routes`
- **`workslate_apply` auto-clears the applied buffer on success** — both from memory and from SQLite. You do not need (and should not) call `workslate_clear` after a successful apply. If apply fails (write error, stale buffer without `force`, unapplied dependency), the buffer is preserved so you can retry. `workslate_clear(name=...)` is only for abandoning a buffer you decided not to apply.
- **Stale buffer detection is on by default.** When `workslate_edit` or `workslate_write` loads a file from disk, the current SHA-256 is recorded. At apply time, if the disk file has changed, apply refuses with an error pointing at `workslate_diff`. If you intentionally want to overwrite the changed file, pass `force=true`. Do not habitually pass `force=true` — it defeats the safety net. Investigate the divergence first.
- **The footer shows staged buffer state.** After each tool call, the footer includes `── Buffers: N staged (names) ──` when any buffer is live. Use this to notice buffers left behind from a prior task and clean them up before starting new work.

## Task Sessions

**`workslate_task_init(name)` is mandatory before using any task tool.** Tasks are stored in SQLite (`workslate.db`) and shared across all agent instances in the same project. This replaces built-in TaskCreate/TaskUpdate entirely.

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
- Buffers are shared across sessions (not scoped)
- Multiple agent instances can read/write the same session concurrently (SQLite WAL mode)

## After Completion

- [ ] All deliverables complete
- [ ] No placeholders or TODOs remain
- [ ] Tests pass (if applicable) — **actually verified, not assumed**
- [ ] No regression in related features
- [ ] Linting/type checking passes (if applicable)
- [ ] No live workslate buffers remain — successful `workslate_apply` auto-clears on success; only abandoned buffers need explicit `workslate_clear(name=...)`
- [ ] Outcome reported faithfully — failures disclosed, not hidden
