---
name: palette-state
description: Keep a palette project's work documents current while working — record a decision, an open question or a discrepancy the moment it happens; move backlog items through their statuses; open a phase and write its deliverables once the user approves them; close a phase. Use when a decision is made, a fact is verified, the user approves or changes a phase or deliverable, or a phase finishes.
---

<!-- slate-agent-kit:common -->
# palette-state

Updates the backlog, phase, deliverable and state documents at the moment something changes, so that a session ending at any point leaves them true.

Use the palette server's write tools when they are available; each one changes every affected file or none and returns the diff. Without the server, edit by hand from the templates in `palette-init/templates/` and keep the same rules.

## When something changes

- **A decision is made** by the user: `palette_state_record` as a decision, one line, with its source and the record or document it will be written into. When that record exists, `palette_state_resolve` it as graduated.
- **A question blocks the active phase**: record it as an open question, without choosing an answer; a proposal is marked with who proposed it and when. When the user answers, resolve it as answered (it becomes a decision).
- **Two sources disagree** (a document and the code, two documents): record a discrepancy with the kind of evidence and the date. Remove it when fixed.
- **Item status changes**: `palette_backlog_update`. Status lives only in the backlog.

## Phases and deliverables

- **Opening a phase** follows the user's approval of its goal and items (§ 5): `palette_phase_open` with goal, reason, assumptions and exit criteria.
- **A deliverable** is written after the user approves its boundaries and outcomes: `palette_deliverable_create`. Its `Done when` states outcomes a person can check against the contract. After approval, changing it is a deviation that needs the user's approval.
- **Closing a phase**, once its exit criteria hold: for each item, `done` with an outcome pointer (the RFC, ADR or changelog entry where the result is recorded) or `dropped`; new problems become proposed items with the user's consent; lessons that are rules become proposed rule text; a broken assumption becomes a discrepancy or an item. Then `palette_phase_close` marks the phase closed; its files stay as the record of what was approved.

## What never goes in

No development stages or progress narrative (what ran when, which model, which batch), no instructions to a later session, no checkboxes, no copy of a fact another document holds. When a sentence would only record history, leave it to version control.
