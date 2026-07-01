---
name: palette-init
description: Opt a project into palette (the product-intent outer loop). Scaffolds the _palette/ backlog through a light intake and asks about git handling. Use when the user wants to start managing a project or roadmap with palette, or accepts the offer to set palette up. Does not scope a phase — the palette loop does that afterward.
---

<!-- claude-agent-kit -->
# palette-init — opt a project into palette

This skill turns palette on for a project: it scaffolds `_palette/` and seeds the backlog through a **light** intake. It does not scope a phase and writes no source code. After init, the palette loop (defined in the always-loaded `claude-agent-kit--palette.md` rule) takes over automatically, because `_palette/` now exists.

Prefix messages with `[palette-init]:`.

## When this runs

The user invoked `palette-init`, or accepted the one-line offer to set palette up. If `_palette/` already exists, do not re-scaffold — report that the project is already palette-active, point at `_palette/backlog.rst`, and stop.

## Step 1 — Light intake

Keep it short: a few questions, not an interrogation. Ask only what changes what gets built.

1. What are you building, in one or two sentences?
2. Who is it for?
3. What platform? (web / mobile / desktop / CLI / library)

Then, only if not already clear: what is the core pain, or why is this worth building? Push back on vague terms ("simple", "basic", "standard") — ask what they concretely mean.

Intake posture — keep it light, but hold these lines:
- Ask **what** the product is, not **how** it is built. Do not ask about or decide stack, storage, auth, or architecture. If the user *volunteers* a technical directive ("use Postgres", "must be offline-first"), transcribe it verbatim into the relevant backlog item's context — do not act on it or evaluate it here.
- Do not propose architecture, libraries, or a decomposition.
- Scope from the user's answers, not by reading the codebase to enumerate work.

## Step 2 — Seed the backlog

Decompose what you learned into backlog items. Use the `_palette/backlog.rst` schema and the RST house style from the always-loaded palette rule (`claude-agent-kit--palette.md`) — do not restate the schema here, follow it. Each discussed feature, requirement, or constraint becomes an item with `:Status: backlog`. Capture genuine durable product principles (product feel, interaction expectations, simplicity / performance / accessibility posture) in the `Core Product Principles` section — 3–7 max, never invented to fill space.

Create the `_palette/` directory and write `_palette/backlog.rst`. Do not create phase folders, stories, or any optional artifact — those come later in the loop.

## Step 3 — git handling (ask)

`_palette/` defaults to **not committed** (see the palette rule). Ask the user:

> Add a `_palette/.gitignore` so palette stays out of git? It is your personal planning record by default. Say no if you want to commit it — e.g. to share across a team.

- Yes → write `_palette/.gitignore` containing a single line: `*`
- No → write nothing; leave `_palette/` untracked for the developer to handle.

Do not commit anything yourself either way.

## Step 4 — Hand off to the loop

Confirm what was created, then hand off. Do not scope a phase here — the loop (now active because `_palette/` exists) owns slicing the first thin phase with the user, when they are ready.

```
[palette-init]: palette is set up.
- _palette/backlog.rst — <N> items seeded
- git: <.gitignore added | left untracked>

Next: when you're ready we'll slice the first thin phase from the backlog. Tell me what you want to tackle first, or keep adding to the backlog.
```

## Forbidden

- Do not write or fix source code — palette-init only scaffolds `_palette/`.
- Do not scope or draft a phase, create stories, or create any optional artifact (tech-spec / ux-flow / design-brief / project-rules) — those belong to the loop and the pull-only `palette-*` skills.
- Do not ask about or decide tech / architecture; transcribe volunteered directives verbatim, do not act on them.
- Do not commit `_palette/` to git.
