<!-- slate-agent-kit:common -->
# palette

palette is a project's document system: backlog, phases and deliverables for work; state for decisions not yet recorded, blocking questions and discrepancies; RFCs and ADRs for decisions with their evidence; changesets and staging for accepted contracts not yet implemented; design, spec, principles and glossary for maintained truth. `_palette/layout.rst` records where each family lives in the project.

## Engagement

The only trigger is whether the project contains `_palette/`.

- **`_palette/` exists: engaged.** Resume from state at the start of work on the project and keep the documents current as decisions are made.
- **`_palette/` absent: dormant.** Do not read, create or mention `_palette/`. Only the user invoking `palette-init` creates it. For work shaped like a project (several increments across sessions) you may offer it in one line; not for a bug fix, a bounded feature or failing CI.

palette documents advise wherever they live; only the user's approval authorizes execution (INV-AUTH-1). A document's placement decides who can see it, never what it authorizes.

## Families

Each family holds one kind of fact, and each fact lives in one place.

- **backlog**: every work item and its status (`proposed`, `approved`, `in-phase-<N>`, `done`, `dropped`); the index of phases and deliverables. Status lives nowhere else.
- **phase**: goal, reason, assumptions, exit criteria of the active increment.
- **deliverable**: one approved unit of the phase and its `Done when`.
- **state**: decisions not yet written into a record, questions that block the active phase, discrepancies between sources; one line each, naming where each will be written.
- **RFC / ADR**: a decision, why it was made, and the evidence it relies on: the current code or document state, the dependency basis in earlier records when nothing exists yet, and research findings.
- **changeset / staging**: an accepted record's edits to maintained documents that the source does not implement yet; staging is generated from them.
- **design / spec / principles / glossary**: the maintained description of the system as the source implements it.

No document records development stages or progress narrative: what was done when, by whom, with which model, in which batch. Version control and session transcripts hold that history. No document instructs a later session; an earlier session's view is a dated `Proposal`.

## Writing the documents

- When the palette MCP server is available, write every palette document through its tools: they allocate identifiers, keep status in the backlog, move decisions into records, apply changesets, regenerate staging and indexes, and change every affected file or none. Pre-approved read tools (`palette_status`, `palette_lint`, `palette_layout`, `palette_template`) need no ceremony.
- Without the server, edit by hand from the templates (`palette_template`, or `$HOME/.claude/skills/palette-init/templates/`) in the RST house style; the next resume's lint reports what drifted.
- Update state when a decision is made or a fact is verified, not at the end of a session: a session can end at any moment.
- Record links point only to older records; `Depends` lists the records whose contract this one uses directly; incoming links, the full closure and supersession are computed, never written.
- An accepted record's body is frozen: a later change of contract is a new record. A maintained document states only what the source implements; the accepted remainder lives in the record's changeset until its implementation lands, when the edits are promoted.

## The loop

1. **Resume** (`palette-resume`): read state within its budget, report the lint result, where things stand, the open questions and a proposal, then wait for the user's direction (INV-DIR-2).
2. **Plan a phase** with the user. Recommend candidates with the judgment criteria (`palette-init/templates/rubrics.rst`); the user chooses. Opening the phase moves its items to `in-phase-<N>`.
3. **Deliverables.** Recommend boundaries; the user approves each. Approval is the step that makes a deliverable authorized work; its `Done when` and `Not this deliverable` are then the spec.
4. **Execute** each deliverable through the execution loop. Record decisions and verified facts in state as they happen (`palette-state`), and write decisions into RFCs and ADRs (`palette-record`).
5. **Close the phase.** Mark each item `done` with an outcome pointer or `dropped`, add new problems as proposed items with the user's consent, propose rules that lessons suggest, record broken assumptions as discrepancies or items, then delete the phase's files.

## Gates under palette

- **GATE-SCOPE-CONFIRM.** Proposing a phase or a deliverable is a checkpoint: report, propose, wait for approval.
- **GATE-DEVIATION.** After approval, moving an unmet `Done when` into the backlog, reordering deliverables, or changing a design needs the user's approval like any deviation.
- **Scope.** palette never shrinks or defers approved scope. A narrower phase needs the user to approve it and to name what moves to the backlog.
- **Delegation.** A delegate receives the approved scope and `Done when`, not the raw palette documents.
- **Git.** Internal families are the user's personal record; never commit them. Families under a project path are committed with the change they describe.
