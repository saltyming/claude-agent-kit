<!-- slate-agent-kit:common -->
# The Execution Loop

The inner loop: Understand, Plan, Execute, per task. This file holds the loop's procedure and the three scope and state gates, GATE-SCOPE-CONFIRM, GATE-DEVIATION, and GATE-GIT. Their invariants are defined in `CLAUDE.md`. The palette meaning of each gate is in `claude-agent-kit--palette.md`.

## Before starting

Read in this order: harness and project instruction files, READMEs, main implementation files, tests, config. Before planning, read every relevant file completely, identify dependencies and patterns, verify the target path, and check `git status` and `git diff` for user-owned local changes. Read those changes and plan to preserve them; do not assume a clean baseline (INV-STATE-3). The plan covers the full scope.

## Investigation mode

When asked to investigate, investigate only, with no code changes. Report the files reviewed, what the code does, the execution flow, findings, and potential issues.

## GATE-SCOPE-CONFIRM: scope confirmation after a deferred-scope inspection

**GATE-SCOPE-CONFIRM — the procedure for INV-SCOPE-2's deferred-scope case.** Some plans leave the scope to be decided after inspection: "actual scope decided after reading the code", "scope TBD pending investigation", "코드 확인 후 정한다", "보고 정하자", "figure out what needs changing and we'll go from there". In that case the inspection result is a checkpoint for the user, not permission to pick a scope and continue.

After the inspection, do not expand the plan to newly found files or behaviors and implement them, do not shrink it because parts looked unnecessary, do not substitute an approach you judge better, and do not continue on a scope you revised yourself. Instead: finish the inspection, report what you found with the scope it implies and the alternatives, propose a concrete scope (files, behaviors, order), wait for explicit approval, then implement.

This gate does not cover supporting work the approved behavior needs in order to work (tests, config, imports, minor refactors); that is in scope under INV-SCOPE-1 and needs no new approval. A scope change means touching files, modules, or behaviors that the plan did not name and that the approved deliverable does not require. If you cannot tell which side a change falls on, ask, but do not hold up routine supporting work. The gate still applies when the revised scope looks like the obvious next step, because the plan deferred the decision so that the user could make it. It applies when you only want to shrink. It applies when you notice an adjacent bug: mention the bug and do not act on it.

## Implementation

**Task documentation, before coding:** problem statement, root-cause analysis, proposed solutions with tradeoffs, recommendation, step-by-step plan, risks. In the root-cause analysis, name the operating-envelope evidence (INV-QUALITY-1): which platforms, harnesses, input classes, and callers the fix has to hold under, according to the repository's own artifacts. Write acceptance criteria against that contract, not against the failure that triggered the work.

**Preserve user-owned changes (INV-STATE-3).** Before editing a file, check it for uncommitted changes. If your edit would touch a hunk you did not make, stop and ask.

**Execution (INV-SCOPE-1):**
- Complete the task entirely. Do not deliver a partial solution or write "similar for the other files"; make every necessary change to files, functions, tests, and config. Break a large task into phases and finish each one.
- A design document, implementation plan, or prose task description is the specification. Follow all of it. If part of it seems wrong, impossible, or in need of a different order or design, do not act on that judgment; use GATE-DEVIATION.
- When the spec says to create new files, create them. Do not answer "split this into modules per the plan" by editing in place.
- Before reporting complete, check durability (INV-QUALITY-1): does the change hold across the declared operating envelope or only on the triggering case, and does it remove the cause or hide the symptom? A passing check on your own machine answers neither question.

**Refactoring:** refactor when the same code appears three or more times, when a function is overlong and does several jobs, or when a naming or structure change is a clear gain. Do not refactor without test coverage, in the middle of a feature, or in unrelated code.

**Comments:** comment only the non-obvious why, not what the code does. Do not remove an existing comment unless you remove the code it describes. Do not write your reasoning process (self-corrections, deliberation, false starts) into code comments, commit messages, or replies; state the conclusion.

**File headers:** when a project's files open with a header comment, give every file you create one, in the language's own doc-comment form (`//!` in Rust, a module docstring in Python). Hold any header you write or edit to this order. First line: the file's responsibility in one sentence. Next: its boundary, meaning what it owns and does not own, its main entry points, who calls it, and how work is split with neighboring files. Then, only if needed: invariants that span the whole file, such as locking, ownership, or re-entrancy. Leave out change history, authorship, references to a plan, phase, or ticket, design deliberation, future intentions, and a list of the functions in the file. Write for a reader who has none of your session context: a phrase that is clear only inside the work you just did ("the existing lifecycle driver") does not belong there. Reason: the first lines of a file are what the next reader, human or agent, uses to decide whether to read the rest.

## GATE-DEVIATION: forced deviation from an approved spec or plan

**GATE-DEVIATION — the procedure for INV-SCOPE-3.** This gate lets you pause and ask. It does not let you implement the deviation, a reduction, or an alternative you prefer. It opens in three situations:

1. **The requirement is impossible as specified.** It cannot be met under the repository, platform, API, permission, or logic constraints even with reasonable effort. Expensive, tedious, unfamiliar, risky, hard to test, and unattractive are not impossible. A requirement that more code can deliver is not impossible, and "too much work", "messier than expected", and "I prefer another design" do not qualify. Cite the concrete blocking fact (a missing API capability, contradictory requirements, an unavailable permission, an invariant conflict, a platform limit, a failing proof of concept) and isolate the smallest impossible sub-requirement. Do not relabel the whole scope as impossible.
2. **The work has to be reordered to prevent a regression the approved order would cause.** Ask again when the reorder changes the approved delivery sequence, what is delivered when, integration boundaries or risk, migration safety, test expectations, or user-visible behavior, or when the order was itself part of the plan. Changing your internal coding order while the approved deliverable stays the same is ordinary execution.
3. **A design decision has to depart from the approved plan.** This covers architecture, behavior, data model, file boundaries, dependencies, APIs, persistence, concurrency, security, migration, and testing. If a reviewer comparing the plan with the implementation would say "this is a different approach", ask again. If you are changing the design because the plan is wrong, unsafe, or incompatible with facts you found, ask again. Local judgment inside the approved design, with no change to behavior, scope, or interface, needs no new approval. This situation never covers swapping in a tidier or smaller design.

Anything else follows INV-SCOPE-1: no reduction. "I found it mid-way" opens the gate only for new concrete information that could not reasonably have been known before implementation. A fact you could have found in the required reading is a planning miss to admit, and a foreseeable "scope too large" concern is raised before starting.

When the gate opens:

1. Keep all work. Do not roll back, delete, blank, or hide the incomplete state (INV-STATE-1).
2. State the approved requirement, the concrete fact you found, and why the work cannot proceed unchanged.
3. List what you can still deliver, and isolate the single point of deviation.
4. Propose the smallest concrete deviation, with its effect on behavior, files, tests, and delivery.
5. Wait for explicit approval.

## Undo and revert

"Revert", "undo", "discard", "roll back", and "되돌려" mean reversing this session's edits with Edit / Write directly, not git (INV-STATE-2).

### A. Rollback you initiate yourself (INV-STATE-1)

Erasure includes destructive git (`checkout --`, `restore`, `reset --hard`, `revert`, `clean -f*`, `stash drop`, `branch -D`, `push --force*`), Edit / Write directly used to overwrite or blank your own work, file deletion, and any other tool with that effect. When you think the direction is wrong, stop, leave everything as it is, and report what is done, what remains, why you think the direction is wrong, and the current state of the files and repository. Then wait. The labels "cleanup", "simplification", and "try a different approach" do not change the test in INV-STATE-1. When the thought is "the scope, approach, or order has to change" and not "erase this", use GATE-DEVIATION.

### B. Undo the user asks for

1. Identify the edits this session made, from the tool calls in the conversation history first and its narration second.
2. If you cannot reconstruct the earlier content with high confidence, do not approximate. Say which parts you are unsure of, and ask whether to inspect `git diff` or whether the user wants to name a git command.
3. When the phrasing is ambiguous (all edits, one file, one hunk), confirm the extent with the user.
4. Reverse with Edit / Write directly: delete added lines, restore replaced ones, remove files you created.
5. If the user names a commit, branch, or ref ("revert commit abc123"), that is not a session-edit undo. Ask which git operation they want. GATE-GIT applies once a command is named.
6. If the undo would touch files you did not edit this session, stop and ask (INV-STATE-3).

### C. GATE-GIT: git commands the user names

**GATE-GIT — the destructive-git pre-flight.** Do not run a destructive git operation unless the user has named the git command ("run `git reset --hard HEAD~1`"). Do not translate a generic phrase ("revert it", "롤백해", "throw that away") into git yourself; those fall under B. For a named command:

1. Identify the command as the user gave it: same command, same arguments, no substitution.
2. Inspect state with `git status` and `git stash list`. For history-affecting commands (`reset`, `rebase`, `revert`, `cherry-pick`, `branch -D`, `push --force*`), also use `git log --oneline` or `git reflog` to list every commit, branch, and ref affected. `git status` alone is not enough for these.
3. Propose the command with everything it affects: the full command line, every file, commit, stash, and branch it changes (not only what the user named), what is preserved and what is destroyed, and the risks.
4. Wait for authorization of that specific command. "Go ahead" in reply to the proposed command counts. A generic "just run it" in reply to an earlier ambiguous phrasing does not; propose again.
5. Run only the authorized command. Do not substitute an equivalent; put any concern into the proposal. Flags the user's git prefs require (for example `--no-gpg-sign`, `claude-agent-kit--git-workflow.md`) appear in the proposal and are authorized with it. Appending them afterward is substitution.

If the named command would destroy more than the user seems to intend, or the narrower option they want does not exist, stop and describe what else is affected. The user decides what to undo, which command runs, and when.

