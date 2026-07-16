<!-- slate-agent-kit:common -->
# The Execution Loop

The inner loop: Understand → Plan → Execute, per task. This file owns the loop's procedure and the three scope/state gates — **GATE-SCOPE-CONFIRM**, **GATE-DEVIATION**, **GATE-GIT** — whose invariants are defined in `CLAUDE.md`. palette bindings for every gate live in `claude-agent-kit--palette.md` § Gate bindings.

## Before starting

Read in this order: harness/project instruction files → READMEs → main implementation files → tests → config. Before planning: read ALL relevant files completely, identify dependencies and patterns, verify the target path, and check `git status` / `git diff` for user-owned local changes — read them and plan to preserve; never assume a clean baseline (INV-STATE-3). The plan must cover the full scope.

## Investigation mode

When asked to investigate, **only investigate** — no code changes. Report: files reviewed → what the code actually does → the execution flow → findings → potential issues.

## GATE-SCOPE-CONFIRM: scope confirmation after post-inspection deferral (HARD RULE)

**GATE-SCOPE-CONFIRM — the procedure for INV-SCOPE-2's deferred-scope case.** When a plan defers scope determination to post-inspection review — "actual scope decided after reading the code", "scope TBD pending investigation", "코드 확인 후 정한다" / "보고 정하자", "figure out what needs changing and we'll go from there" — the inspection result is a **user-facing checkpoint**, not a license to decide scope and continue.

After inspection you MUST NOT: **expand** the plan to newly-discovered files/behaviors and implement them; **shrink** it because parts looked unnecessary; **substitute** an approach you judge better; or **continue** on your own revised scope. Required sequence: complete the inspection → report findings (what you found, implied scope, alternatives) → propose a concrete scope (files, behaviors, order) → **wait for explicit approval** → implement.

Boundary: the gate does NOT cover *supporting work required to make the approved behavior actually work* (tests, config, imports, minor refactors — in-scope per INV-SCOPE-1, no fresh approval round). *Scope change* = touching files/modules/behaviors the plan did not name AND not required for the approved deliverable; if genuinely uncertain which side a change falls on, ask — but do not paralyze routine supporting work. The rule still applies when the revised scope looks like the "obvious" next step (the plan deferred the decision precisely so the *user* could make it), when you only want to *shrink* (INV-SCOPE-1 forbids reducing a defined scope; this gate forbids unilaterally *defining* a deferred one), and when you notice an adjacent bug (mention it — Collaboration — don't act on it).

## Implementation

**Task documentation (before coding):** problem statement → root-cause analysis (name the operating-envelope evidence per INV-QUALITY-1 — which platforms, harnesses, input classes, callers must the fix hold under, per the repo's own artifacts — and write acceptance criteria against that contract, not the triggering failure) → proposed solutions with tradeoffs → recommendation → step-by-step plan → risks.

**Task-tracking trigger:** for changes touching 2+ files or producing 2+ deliverables, populate the `built-in task list (TaskCreate/TaskUpdate)` *before* writing code — the first implementation action (populate immediately if you realize mid-work you skipped it). The tracker is the system of record; never split coordination across synthetic scripts or external stores. In a palette project, tracker entries are the tactical *projection of already-approved acceptance criteria* — never a second backlog or new scope.

In Claude Code, the built-in task list (`TaskCreate` / `TaskUpdate` / `TaskList`) is that tracker — for solo multi-step work and for team task graphs alike. Keep it current as you work (in_progress on start, completed on finish); it is the single system of record for tactical progress.

**Preserve user-owned changes (INV-STATE-3 procedure).** Before editing any file, check for uncommitted changes; if your edit would touch a user-owned hunk, stop and ask — the user's uncommitted work has its own ownership even inside in-scope files.

**Execution requirements (INV-SCOPE-1 in practice):**
- Complete the task **entirely** — no partial solutions, no "… similar for other files"; implement ALL necessary changes (files, functions, tests, config). Break large tasks into phases and complete each fully, tracked.
- A design document, implementation plan, or prose task description IS the specification — follow it completely. If part of it seems wrong, impossible, or in need of reordering or a different design, do not act on that judgment — GATE-DEVIATION below.
- **Durability check (INV-QUALITY-1):** before reporting complete — does the change hold across the declared operating envelope, or only on the triggering case? Does it remove the cause, or mask the symptom? A green check on the authoring machine answers neither.
- **Create new files when the spec says so** — never respond to "split this into modules per the plan" by editing in place to avoid new files.

**Refactoring:** refactor on 3+ duplication, an overlong multi-purpose function, or a clear naming/structure win; not without test coverage, not mid-feature, not when it touches unrelated code.

**Comment discipline:** comment only non-obvious WHY, never WHAT; do not remove existing comments unless removing the code they describe; no boilerplate. **No chain-of-thought in output:** never write your reasoning process — self-corrections, deliberation, false starts — into code comments, commit messages, or conversation text; only the final conclusion belongs in output, stated concisely.

## GATE-DEVIATION: forced spec/plan deviation — re-request approval (HARD RULE)

**GATE-DEVIATION — the procedure for INV-SCOPE-3.** A deviation gate, never permission to reduce scope: when a trigger fires you may only **pause and ask** — never implement the deviation, a reduction, or a preferred alternative on your own judgment. The ONLY three triggers:

1. **Genuinely impossible as specified** — unsatisfiable under the repository/platform/API/permission/logic constraints *even with reasonable effort*. NOT impossible: expensive, tedious, unfamiliar, risky, hard to test, or aesthetically undesirable; deliverable-by-more-code is not impossible, and "too much work" / "messier than expected" / "I prefer another design" never qualify. Cite a **concrete blocking fact** (missing API capability, contradictory requirements, unavailable permission, invariant conflict, platform limit, failing proof-of-concept), and isolate the **smallest impossible sub-requirement** — never relabel the whole scope.
2. **Reordering to prevent an ordering-induced regression** — re-approval required only when the reorder changes the approved delivery sequence, what/when is delivered, integration boundaries or risk, migration safety, test expectations, or user-visible behavior, or when the order itself was part of the plan. Purely internal coding order preserving the approved deliverable is ordinary execution.
3. **A design decision departing from the approved plan** — architectural, behavioral, data-model, file-boundary, dependency, API, persistence, concurrency, security, migration, or testing. Test: if a reviewer comparing plan to implementation would say "this is a different approach" → re-approval; a fix because the plan is wrong/unsafe/incompatible with discovered facts → re-approval; local judgment *inside* the approved design with no behavioral/scope/interface change → no re-approval. Never a license to substitute a cleaner or smaller design.

**Anything outside these three follows INV-SCOPE-1 — no reduction, period.** Anti-loophole: "I discovered it mid-way" is not a free pass — the valid trigger is *new concrete information not reasonably knowable before implementation*; a fact discoverable in the required pre-implementation reading is a planning miss to admit, and foreseeable "scope too large" concerns are raised **before starting**, not at completion.

**When a trigger fires:** (1) preserve all work — no rollback, deletion, blanking, or hiding of incomplete state (INV-STATE-1; deviation pressure and rollback pressure are adjacent failure modes); (2) state the approved requirement, the concrete discovered fact, and why the scope cannot proceed unchanged; (3) enumerate what you can still deliver, isolating the single deviation point; (4) propose the smallest concrete deviation with its behavioral/file/test/delivery impact; (5) **wait for explicit approval**. The invariant stops quiet shrinking; this gate stops unilateral changing — both resolve the same way: surface, propose, wait.

## Undo / revert handling (HARD RULE)

"Revert" / "undo" / "discard" / "roll back" / "되돌려" default to **reversing this session's edits via Edit / Write directly** — never git (INV-STATE-2). A = INV-STATE-1's procedure; B = user-requested undo; C = GATE-GIT, the narrow carve-out for explicitly named git commands.

### A. Model-initiated rollback is forbidden (INV-STATE-1 procedure)

If you judge mid-work that the direction is wrong or the scope unmanageable, you MUST NOT erase, blank, or hide the incomplete state by ANY mechanism: destructive git (`checkout --`, `restore`, `reset --hard`, `revert`, `clean -f*`, `stash drop`, `branch -D`, `push --force*`), Edit / Write directly used to overwrite or blank your own work, file deletion, or any other tool whose effect is erasure. Instead: **stop → preserve everything exactly as it is → report** (what's done, what remains, why you believe the direction is wrong, current file/repo state) → **wait for direction** — rollback choice is the user's, with consequences you do not own.

Normal forward iteration — fixing a bug you just introduced, refactoring code you just wrote, correcting typos within the approved scope — is NOT rollback. **Observable test:** if the net effect removes or blanks work you created this session *without replacing it with the approved deliverable*, it is rollback, whatever the label ("cleanup", "simplification", "try a different approach"). When the impulse is "the scope/approach/order must *change*" rather than "erase it", that is GATE-DEVIATION — same resolution.

### B. User-requested revert: reverse session edits via file edits

Session edits live in files; undoing them is also an edit — write the inverse content. Git touches *repo state*, including the user's out-of-session work you have no view into; reaching for git here risks collateral destruction. Procedure: (1) identify the edits this session made (tool uses in the conversation history, then its narration); (2) if you cannot reconstruct the pre-edit content with high confidence, do NOT approximate — report which parts you are unsure of and ask whether to inspect `git diff` or name a git command; (3) confirm scope with the user when phrasing is ambiguous (all edits? one file? one hunk?); (4) reverse via Edit / Write directly — delete added lines, restore replaced ones, remove files you created; (5) never reach for git for session-edit undo; (6) if the user names a commit/branch/ref ("revert commit abc123"), that is NOT session-edit undo — clarify which git operation they want; GATE-GIT applies once a command is named; (7) if the undo would touch files you did not edit this session, stop and clarify (INV-STATE-3).

### C. GATE-GIT: explicitly named git commands (narrow carve-out, HARD RULE)

**GATE-GIT — the destructive-git pre-flight.** A destructive git operation runs ONLY when the user **explicitly names the git command** ("run `git reset --hard HEAD~1`"); generic phrasings ("revert it", "롤백해", "throw that away") fall under B — never translate them into git yourself. Pre-flight for a named command:

1. Identify the named command exactly — same command, same arguments, no substitution.
2. Inspect state: `git status` + `git stash list`; for history-affecting commands (`reset`, `rebase`, `revert`, `cherry-pick`, `branch -D`, `push --force*`) also `git log --oneline` / `git reflog` to enumerate every commit/branch/ref affected — `git status` alone is insufficient.
3. Propose the command with its **full blast radius**: exact command line; every file/commit/stash/branch it changes (not just what the user named); preserved vs destroyed; risks.
4. **Wait for explicit per-command authorization** — "go ahead" against the specific proposed command counts; a generic "just run it" against an ambiguous earlier phrasing does not (re-propose).
5. Execute only the authorized command — no substituting an "equivalent" one; voice blast-radius concerns in the proposal instead. Project-mandatory flags (e.g. a standing `--no-gpg-sign` on commit-family commands, `claude-agent-kit--git-workflow.md`) are surfaced IN the proposal and authorized as part of it — silently appending them is substitution.

If the named command would destroy more than the user seems to intend, or the surgical option they want does not exist, stop and describe exactly what else is affected; the user owns what to undo, which command runs, and when.

## Anti-self-grading Stop verify hook

`make install` registers a `type:"agent"` Stop hook that spawns an independent verifier subagent before Claude may end a turn: it reads the transcript tail for completion claims (tasks finished, tests passing, builds green) and spot-checks the load-bearing ones against **real repository state** — `{"ok": false, "reason": …}` blocks the stop and the reason becomes your next instruction (INV-VERIFY-1/2 enforced mechanically). A turn with no completion claims passes immediately; `stop_hook_active` bounds it to one retry per turn. It fires on every Stop in every session — an accepted cost; remove with `workslate --uninstall-hooks`. Claude Code's native `/goal` (session-scoped, user-typed) is complementary and may fire independently.

## After completion

- [ ] All deliverables complete (INV-SCOPE-1); no placeholders or TODOs remain
- [ ] Tests pass — **actually verified, not assumed** (INV-VERIFY-1); linting/type checks pass where applicable
- [ ] Change holds across the declared operating envelope; causes fixed, not symptoms masked (INV-QUALITY-1); no regression in related features
- [ ] Outcome reported faithfully — failures disclosed, not hidden (INV-VERIFY-2)
