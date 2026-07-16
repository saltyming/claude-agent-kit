---
name: palette-init
description: Opt a project into palette (the product-intent outer loop). Scaffolds the _palette/ backlog and the on-disk template pack (_palette/templates/) through a light intake, and asks about git handling. Use when the user wants to start managing a project or roadmap with palette, accepts the offer to set palette up, or when an existing palette project is missing _palette/templates/. Does not scope a phase — the palette loop does that afterward.
---

<!-- slate-agent-kit:common -->
# palette-init — opt a project into palette

This skill turns palette on for a project: it scaffolds `_palette/` (including the on-disk **template pack** the loop reads when writing artifacts) and seeds the backlog through a **light** intake. It does not scope a phase and writes no source code. After init, the palette loop (defined in the always-loaded `claude-agent-kit--palette.md` rule) takes over automatically, because `_palette/` now exists.

Prefix messages with `[palette-init]:`.

## When this runs

The user invoked `palette-init`, or accepted the one-line offer to set palette up. If `_palette/` already exists: do not re-scaffold the backlog — but if `_palette/templates/` is missing (a pre-v11 palette project), backfill it from the Template pack below, report that, and stop. Otherwise report that the project is already palette-active, point at `_palette/backlog.rst`, and stop.

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

## Step 2 — Scaffold the template pack

Create `_palette/templates/` and write each file from the **Template pack** section below, verbatim. These are what the loop consults when writing any artifact — the always-loaded rule deliberately does not restate them.

## Step 3 — Seed the backlog

Decompose what you learned into backlog items using `_palette/templates/backlog.rst` as the shape and `_palette/templates/house-style.rst` as the syntax rules. Each discussed feature, requirement, or constraint becomes an item with `:Status: backlog`. Capture genuine durable product principles (product feel, interaction expectations, simplicity / performance / accessibility posture) in the `Core Product Principles` section — 3–7 max, never invented to fill space.

Write `_palette/backlog.rst`. Do not create phase folders, stories, or any optional artifact — those come later in the loop.

## Step 4 — git handling (ask)

`_palette/` defaults to **not committed** (see the palette rule). Ask the user:

> Add a `_palette/.gitignore` so palette stays out of git? It is your personal planning record by default. Say no if you want to commit it — e.g. to share across a team.

- Yes → write `_palette/.gitignore` containing a single line: `*`
- No → write nothing; leave `_palette/` untracked for the developer to handle.

Do not commit anything yourself either way.

## Step 5 — Hand off to the loop

Confirm what was created, then hand off. Do not scope a phase here — the loop (now active because `_palette/` exists) owns slicing the first thin phase with the user, when they are ready.

```
[palette-init]: palette is set up.
- _palette/backlog.rst — <N> items seeded
- _palette/templates/ — artifact template pack written
- git: <.gitignore added | left untracked>

Next: when you're ready we'll slice the first thin phase from the backlog. Tell me what you want to tackle first, or keep adding to the backlog.
```

## Forbidden

- Do not write or fix source code — palette-init only scaffolds `_palette/`.
- Do not scope or draft a phase, create stories, or create any optional artifact (tech-spec / ux-flow / design-brief / project-rules) — those belong to the loop and the pull-only `palette-*` skills.
- Do not ask about or decide tech / architecture; transcribe volunteered directives verbatim, do not act on them.
- Do not commit `_palette/` to git.

---

## Template pack

Write each fenced block below to the named file, exactly as given (replace nothing at scaffold time — `<...>` / `[...]` fill-ins are for the loop to fill when it writes real artifacts).

### `_palette/templates/house-style.rst`

```rst
RST house style — robust subset
===============================

palette artifacts are structured text the agent reads back, not documents
rendered by Sphinx/docutils. Consistent structure and greppability are the
goal, not strict RST validity. LLMs malform elaborate RST silently, so use
ONLY this subset:

- Field lists for metadata: ``:Type: feature``, ``:Status: backlog``.
- Plain bullet lists (``-``) and definition lists.
- Literal ``- [ ]`` / ``- [x]`` for acceptance-criteria checkboxes (no RST
  meaning; grep-compatible).
- Section titles underlined with ``=`` (H1), ``-`` (H2), ``~`` (H3); make the
  underline at least as long as the title.
- Do NOT use ``.. list-table::``, grid/simple tables, or any nested directive
  — these are the constructs models most often break.
```

### `_palette/templates/backlog.rst`

```rst
Backlog — <Project>
===================

Core Product Principles
-----------------------

- <durable principle that should outlive any single phase>

Items
-----

Each item uses the shape below. Do not group items into phase sections and do
not use checkboxes here. An item's :Status: moves to ``in-phase-<N>`` only
after the user approves that phase's scope. ``:Priority-signal:`` is optional,
set only by review-triage; it is Tier-A bookkeeping and never moves :Status:
on its own.

<Short title>
~~~~~~~~~~~~~

:Type: bug | refinement | feature | tech-debt | test | spec-gap | rule-gap | chore
:Source: Phase <N> | User idea | Review triage
:Status: backlog | in-phase-<N> | resolved
:Priority-signal: high | medium | low | blocked | decision-gate

<What it is; why it matters — one or two lines.>
```

### `_palette/templates/phase-brief.rst`

```rst
Phase Brief — <Project> — Phase <N>
===================================

:Status: active | closed

Why This Phase
--------------

<One or two sentences: why now, what it makes possible.>

Phase Goal
----------

<One sentence: the single most important outcome. If scope is cut, this survives.>

Scope
-----

- <what ships — one line per item>

Exit Criteria
-------------

- <what a real person can do when this phase is done>

Assumptions
-----------

- <assumption> — risk if wrong: <risk>

Stated Technical Preferences
----------------------------

<Include ONLY if the source explicitly stated a stack / framework / storage /
auth directive. Transcribe verbatim, one per line. Omit this section entirely
otherwise; never invent one.>
```

### `_palette/templates/story.rst`

```rst
STORY-<n>: <Short title>
========================

.. filename: story-<n>-<slug>.rst — slug lowercase-hyphenated, 2–4 words.
   Story numbers are the build order.

What and why
------------

<2–3 sentences: the specific persona, what changes for them, why it matters.
Not "as a user".>

Done when
---------

- [ ] <observable behaviour a non-developer can verify>

Not this story
--------------

- <explicit scope boundary>

Implementation Reference
------------------------

- <terse pointers: files, constants, boundaries — only what the implementer
  cannot easily find. No prose, no hedged paths.>
```

### `_palette/templates/index.rst`

```rst
Stories — <Project> — Phase <N>
===============================

:1: <title> — story-1-<slug>.rst — pending
:2: <title> — story-2-<slug>.rst — pending

.. The status field (pending / done) is the single source of story state:
   select the next pending story from it, and record done there on completion
   (Tier A bookkeeping — recording completion, never reopening scope).
```

### `_palette/templates/reviews.rst`

```rst
Reviews — <Project>
===================

Phase <N>
---------

:Closed: <date or "session">

What worked
~~~~~~~~~~~

- <observation>

What didn't
~~~~~~~~~~~

- <observation>

Assumption results
~~~~~~~~~~~~~~~~~~~

- <assumption from the brief> — held | broke: <what happened>

Backlog triage
~~~~~~~~~~~~~~

- <new/deferred item routed to backlog.rst, WITH user consent>
```

### `_palette/templates/rubrics.md`

```markdown
# palette scoring rubrics

Three loop decisions get an explicit, reviewable heuristic. They RANK and
SURFACE; they never decide — every output is a recommendation the user can
override, and none of them moves a `:Status:` field. Score each axis 1 (low) /
2 (medium) / 3 (high) from the item's title, `:Type:`, `:Source:`, and body
prose. Output is a tier spoken in conversation — for (c) only, a
`:Priority-signal:` field. Never write scores into artifacts.

## (a) Next-slice ranking — which backlog item(s) to recommend next

Only items with `:Status: backlog`. Screen first — do not score:
- **Blocked** — body names an external dependency the developer cannot
  presently resolve. Surface as "blocked — not a candidate until unblocked".
- **Decision-gate** — body is a yes/no or risk-acceptance question, not
  buildable work. Surface as "needs a user decision before it can be a slice".

Score = Impact×3 + Thin-slice fit×2 + Readiness & leverage×2 + Recurrence×1
(range 8–24).
- Impact: how much shipping this improves the product / trust / live risk. A
  prior `:Priority-signal:` is a starting anchor only; never write back.
- Thin-slice fit (inverted on purpose): 3 = single well-scoped change,
  1 = open-ended multi-area work.
- Readiness & leverage: fully specified now, and unblocks/de-risks others.
- Recurrence: 2–3 ONLY on an explicit resurfacing cue in the text ("again",
  "still", "outstanding", a second item on the same fix); default 1.

Bands: 17–24 strong next-slice candidate · 11–16 candidate · 8–10 not yet.
Conversational and ephemeral — never written into backlog.rst.

## (b) Story-boundary — own story, or fold into a sibling

Drafting-time only (loop step 3, before hand-off). Never re-run on an
approved story. An unresolved decision ("pick a library first") is not
buildable — route it back to the backlog; do not score.

Score = Independent verifiability×3 + Distinct footprint×2 + AC substance×2 +
Sequencing constraint×1 (range 8–24).
- Independent verifiability: `Done when` checkable by a non-developer with no
  sibling chunk done.
- Distinct footprint: touches files/components no sibling touches.
- AC substance: ≥2 meaningful `Done when` criteria of its own.
- Sequencing constraint: a hard build-order relation a merged story would
  obscure.

Bands: 17–24 own story · 11–16 borderline — fold into the closest sibling,
noting why · 8–10 fold. A chunk whose own `Done when` runs past ~5 criteria is
two candidate chunks — score each separately.

## (c) Review-triage priority — the signal a new/deferred item carries

Loop step 5, before writing an item into backlog.rst (with user consent).
Reuse (a)'s blocked / decision-gate carve-outs — if either applies, write that
label into `:Priority-signal:` instead of scoring.

Score = Impact if unresolved×3 + Blocks current commitments×2 + Recurrence×2 +
Cost-to-fix×1-inverted (range 8–24).
- Impact if unresolved: correctness/trust/safety damage vs cosmetic.
- Blocks current commitments: endangers the closing phase's exit criteria or
  in-flight work.
- Recurrence: has this class of issue surfaced before (weighted higher here —
  noticing repetition is what review triage is for).
- Cost-to-fix inverted: 3 = cheap, 1 = open-ended.

Bands as (a): 17–24 high · 11–16 medium · 8–10 low. Write the label into the
new item's `:Priority-signal:` — Tier-A bookkeeping, never a `:Status:`
promotion. Only this rubric writes `:Priority-signal:`; refresh it at each
later review.
```
