---
name: palette-init
description: Set up palette in a project, or move an older palette project to the current document families. Asks what the project is, where each document family should live, and seeds the backlog. Use when the user asks to start palette, accepts the offer to set it up, or has a `_palette/` folder without `layout.rst`. Does not plan a phase.
---

<!-- slate-agent-kit:common -->
# palette-init

Sets palette up for a project: the layout of its document families, an empty state, and a backlog seeded from a short conversation. It writes no source code and plans no phase; the palette rule takes over once `_palette/layout.rst` exists.

Templates for every family are in this skill's `templates/` folder and through the `palette_template` tool. `templates/house-style.rst` defines the RST subset every palette document uses.

## When it runs

- The user invoked it or accepted the offer: set up a new project.
- `_palette/` exists with `layout.rst`: the project is already set up; say so, point at the backlog, and stop.
- `_palette/` exists without `layout.rst`: the project uses the earlier palette format; propose the move described under *Moving an earlier palette project* and wait for approval.

## Intake

A few questions, only what changes what gets built: what the project is, who it is for, and what platform it targets; then, only if it is not clear, why it is worth building. Ask what the project is, not how to build it: do not ask about or decide stack, storage or architecture. A technical directive the user volunteers is written into the relevant backlog item verbatim. Push back on vague terms ("simple", "standard") by asking what they mean concretely.

## Placement

Every family is used; the user decides where each lives. Propose a layout, then ask:

- work families (backlog, phase, deliverable, state): internal (`_palette/`, the user's personal record, not committed);
- records and maintained documents (rfc, adr, changeset, staging, design, spec, principles, glossary, contributing): the project's existing documentation folder when it has one (reuse its subfolders, numbering and conventions), else `docs/`; internal if the user does not want them shared;
- the project's own document checker, if it has one, as the layout's `checker`. When older records carrying `Amends` are moved in, the contributing document's `Records` section states `:Amends: until <date>`; otherwise it says `none`.

Say that internal documents are git-ignored and never committed, and that documents under a project path are committed with the change they describe.

## Create

With the palette server: `palette_init` with the chosen placements, then one `palette_backlog_add` per item. Without it: write `_palette/.gitignore` (`*`), `_palette/layout.rst`, `_palette/backlog.rst` and `_palette/state.rst` from the templates, and create the project folders the layout names.

Seed the backlog from the intake: each discussed feature, requirement or constraint becomes one `proposed` item. Durable product principles, three to seven at most and never invented to fill space, go into the principles document.

## Report

List what was created and where each family lives, then stop: planning the first phase is the palette loop's next step, when the user is ready.

## Moving an earlier palette project

An earlier palette project has phase briefs, `stories/` with an `index.rst`, `reviews.rst` and a `templates/` folder. Propose the move as a list and wait for approval; apply only what the user approves, after copying `_palette/` to a backup outside the repository.

- `phase-brief.rst` becomes `phase.rst` (goal, reason, assumptions, exit criteria; its scope list becomes backlog items).
- Each `story-<n>-<slug>.rst` becomes `deliverables/deliverable-<n>-<slug>.rst`; its checkboxes become plain `Done when` outcomes.
- The status in `stories/index.rst` becomes each item's backlog status; the index is removed.
- `reviews.rst`: unresolved items become proposed backlog items with the user's consent; the rest is removed.
- Decisions recorded in a state or handoff document become state entries of one line each, pointing to the record they will be written into.
- `templates/` is removed; the templates now ship with the kit.
