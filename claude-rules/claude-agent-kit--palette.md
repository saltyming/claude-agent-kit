<!-- claude-agent-kit -->
# palette — Product-Intent Outer Loop

palette is an **outer loop** that wraps this framework's existing **inner loop** — the Three-Phase Workflow (Understand → Plan → Execute) plus workslate tasks and dispatch/Agent-Team delegation. The inner loop finishes one task correctly and then everything about it evaporates at the task boundary: there is no durable, cross-session record of *what we are building and why*, and no re-planning cadence. palette adds exactly that missing layer above the inner loop — a durable backlog of intent, a thin-slice step that feeds the inner loop one increment at a time, and a review step that harvests each completion back into the backlog.

palette is a **loop, not a product-planning suite**. It is: durable backlog → thin slice → hand off → review → re-plan, expressed in this framework's own terms (agent-loop design + harness-assist tooling, alongside workslate and dispatch). Design artifacts (tech spec, UX flow, UI/design brief, project rules) are **optional, pull-only helpers** — the `palette-*` skills — never part of the default loop.

## Engagement — folder-gated (HARD)

palette's trigger is the presence of a `_palette/` directory in the project. There is no separate progress file and no heuristic guessing about "is this multi-increment enough".

- **`_palette/` exists → engaged.** You are in a palette project. On non-trivial work, weave the outer loop into your normal inner loop *without being asked*: consult `_palette/backlog.rst` for product intent as you plan, and update it as work completes. This is the always-on behavior — but only inside a project that has opted in.
- **`_palette/` absent → dormant.** Do not read, create, update, or mention `_palette/`. One exception: if the work is genuinely project- or roadmap-shaped (multi-increment, spans sessions, framed by the user as "a project / MVP / phase / milestone / roadmap"), you may **offer one line** — "Want me to set up palette for this? Run the `palette-init` skill." — and nothing more. Do not scaffold on your own. For a single bug fix, a bounded one-off feature, a lone refactor, or a failing CI, do not even offer.
- **Opt-in is explicit: the `palette-init` skill.** Only the user running `palette-init` creates `_palette/`. palette never ambushes a project.
- **The distinction is multi-increment, not multi-file.** Multi-file work already belongs to the inner loop; palette wakes only when *product intent and sequencing* are part of the task.

### Auto-engagement ≠ auto-authority (HARD)

The folder's presence switches on the *advisory* loop — it does **not** grant execution authority. Consulting the backlog and stories informs your planning; it never authorizes an edit. Every file change still passes the existing approval gate (below). "The backlog says so" is not permission to edit.

## Authority firewall — Tier A / Tier B (HARD)

palette adds a durable record of intent. To keep it from silently becoming a second, competing definition of "current scope" — which would undercut this framework's scope-integrity rules — the authority split is strict:

- **Tier A — the palette artifacts** (everything under `_palette/`: `backlog.rst`, `phase-brief.rst`, `story-*.rst`, `index.rst`, and any optional `tech-spec.rst` / `ux-flow.rst` / `design-brief.rst` / `project-rules.rst`). **Advisory only.** They *propose* scope; they never *authorize* an edit. **The backlog is an intent ledger, not an execution authority.** A backlog item may only inform questions and proposals. It becomes executable only after it is promoted into a current story/plan, approved by the user, and tracked with workslate/dispatch.
- **Tier B — the existing execution machinery.** The user's approval at the generic "Get approval" gate *authorizes*; workslate `ws:`/`team:` tasks and dispatch specs *track*. These are binding.

**Precedence, highest first:** (1) the current turn's explicit user instruction → (2) the current Tier-B approved-and-tracked scope (what execution actually follows) → (3) a story's acceptance criteria / "Not this story" → (4) the phase brief (context) → (5) the backlog (future intent, explicitly *not* current scope).

### Scope invariant — palette never changes delivery scope (HARD)

The single most important safety rule, and it runs in **both** directions:

- palette **never shrinks or defers the current requested scope.** If the user asked for X, X remains the requested scope. It may be narrowed to a thinner slice **only** when the user explicitly approves the narrower slice *and* explicitly acknowledges which parts move to the backlog. Moving approved acceptance criteria, approved plan items, required tests/config/docs, or implementation-needed support work into the backlog is scope reduction and is forbidden — governed by the existing no-shrink rules and the Forced Spec/Plan Deviation gate in `claude-agent-kit--task-execution.md`.
- **Consent must be specific.** "Approve phase 1?" is not enough. "Approve doing A now and deferring B and C to the backlog?" — the deferred parts named — is what you need.
- **Completion-time is the danger zone.** During execution or the review step you may **not** move approved scope / unmet acceptance criteria into the backlog without explicit user consent. Declaring a story "done" while quietly reclassifying an unmet criterion as "future" is the worst failure this rule exists to prevent.

### Deviation and ambiguity — reuse the existing rules

palette invents no new deviation vocabulary. Once a story has passed the hand-off approval gate, its `Done when` + `Not this story` **are** the "approved spec/plan" that `claude-agent-kit--task-execution.md` → *Forced Spec/Plan Deviation: Re-request Approval* governs: if delivering the story as written turns out genuinely impossible, requires reordering that changes the approved sequence, or needs a design that deviates from the plan — stop, preserve work, propose, wait. Plain undecided ambiguity is handled by the existing Clarification heuristic (ask about WHAT, decide HOW yourself).

## The loop

When engaged (`_palette/` present):

1. **Backlog** (`_palette/backlog.rst`) — a single durable file: the cross-session ledger of product intent (durable product principles + items, each with `:Type:` / `:Source:` / `:Status:`). Advisory (Tier A).
2. **Slice → phase** — with the user, cut the next thin increment from the backlog into a phase (`_palette/phase-<N>/phase-brief.rst`, with entry/exit criteria); rank candidates first with the next-slice rubric (Scoring rubrics, below). This is the user-owned scope decision the framework already mandates.
3. **Stories** — decompose the phase into implementable stories with acceptance criteria (`_palette/phase-<N>/stories/story-<n>-<slug>.rst` + `index.rst`); use the story-boundary rubric (below) to recommend where one story ends and the next begins. Advisory.
4. **Hand off** — at the existing "Get approval" gate. A pending story's acceptance criteria feed the normal inner loop (Understand → Plan → Execute + workslate/dispatch). **Nothing is edited before this gate** — it is the Tier A → Tier B transition.
5. **Review + re-plan** — when a story/phase completes, step back: what shipped, what was learned, what surfaced. Record it (`_palette/reviews.rst` at phase close) and triage new/deferred items into the backlog **with explicit user consent**, scoring each with the review-triage rubric (below) first. Then slice the next increment.

### The combined loop — how it sits on the inner loop

palette does not replace the inner loop; it **wraps** it. The Three-Phase Workflow, workslate, and dispatch are unchanged. The two loops meet at exactly two points: the **hand-off gate** (a story's acceptance criteria enter Understand → Plan → Execute, via the existing "Get approval" node) and the **completion harvest** (the inner loop's verified result returns to the review / backlog update).

```
OUTER (palette · strategic · cross-session)
  consult backlog.rst
  [1] SLICE    -> phase-brief.rst          (Tier A · user-owned scope)
  [2] STORIES  -> stories/*.rst + index    (Tier A · advisory)
  per pending story:
      +-- INNER (existing Three-Phase · unchanged) --------------+
      |  UNDERSTAND  read files (+ preserve user-owned changes /  |
      |              staleness check)                            |
      |  PLAN        story AC -> task doc / (large -> Plan Mode)  |
      |  == GET APPROVAL ==  <- Tier A->B. No edit before here.   |
      |  EXECUTE     workslate task (projection of approved AC,   |
      |              not new scope) + optional dispatch / Agent  |
      |              Team + verify (test/build)                  |
      +----------------------------------------------------------+
      record story done in index.rst (bookkeeping) + one-line report
  [3] REVIEW   -> reviews.rst; triage new/deferred into backlog (with consent)
  [4] RE-PLAN  -> next phase
```

Where the existing primitives plug in: **workslate task** = the inner Execute's tactical projection of an approved story's AC (a translation, not a second backlog); **dispatch / Agent Team** = inner execution delegation, unchanged — the *approved scope*, not the raw story artifact, becomes the dispatch spec / team task; **native memory** = outside the loop (facts, not intent).

## backlog vs native memory — routing

Both persist across sessions; keep them distinct. Evolving intent / scope / deferrals → `_palette/backlog.rst`. Stable facts about the user, project, or a settled decision → native memory (`memory/` + `MEMORY.md`). Never record the same thing in both.

## `_palette/` and git

Default: **not committed.** The backlog is the developer's personal planning record, not shared team state; its cross-session value holds from being on disk in the working copy, and git is not required for that. `palette-init` *asks* whether to add a self-contained `_palette/.gitignore` (contents `*`); a developer who wants clone/team sharing wires up committing themselves. The framework does not commit `_palette/` on its own.

## RST house style — robust subset (read before writing any artifact)

palette artifacts are **structured text the agent reads back**, not documents rendered by Sphinx/docutils. The goal is consistent structure and greppability, not strict RST validity; title-underline length and the like are cosmetic here. LLMs malform elaborate RST silently, so the house style is a deliberately small subset:

- Field lists for metadata: `:Type: feature`, `:Status: backlog`.
- Plain bullet lists (`-`) and definition lists.
- Literal `- [ ]` / `- [x]` for acceptance-criteria checkboxes (no RST meaning; grep-compatible).
- Section titles underlined with `=` (H1), `-` (H2), `~` (H3); make the underline at least as long as the title.
- **Do not** use `.. list-table::`, grid/simple tables, or any nested directive — these are the constructs models most often break.

## Scoring rubrics — slice, story-boundary, triage

Three loop decisions are currently free-form judgment calls: which backlog
item(s) to slice next (step 2), whether a chunk of phase work deserves its own
story (step 3), and what priority signal a newly-triaged item carries into the
backlog (step 5). The rubrics below turn each into an explicit, reviewable
heuristic the model applies by hand from what a backlog item, phase brief, or
story draft already contains — no new tooling, no data that doesn't exist, and
no claim of true determinism (the axes are estimated from prose, not measured;
the value is a named, arguable basis for the ranking, not a repeatable
computation). They **rank and surface; they never decide.** Every output is a
recommendation the user can override, and none of them moves a `:Status:`
field — that stays gated on user approval exactly as the loop already
requires. Because the RST house style above forbids tables, every rubric's
output collapses to a single value — a tier spoken in conversation, or (for
(c) only) a `:Priority-signal:` field — never a matrix; the weights and
formula live here, in prose, as instructions the model re-applies each time,
not as a cached score.

Score each axis `1` (low) / `2` (medium) / `3` (high) from the item's title,
`:Type:`, `:Source:`, and body prose — the same read-and-estimate the model
already does, made explicit and weighted.

### (a) Next-slice ranking — which backlog item(s) to recommend for the next phase

Apply only to items with `:Status: backlog` (skip `in-phase-<N>` and `resolved`).
Before scoring, screen for two carve-outs a numeric score would misrank:

- **Blocked** — the body names an external dependency the developer cannot
  presently resolve (no test environment, "deferred to CI", waiting on a third
  party). Do not score it — a high-impact blocked item would otherwise still
  post a deceptively high composite. Surface it as *blocked — not a candidate
  until unblocked*.
- **Decision-gate** — the body is a yes/no or risk-acceptance question ("decide
  whether…", "accept or mitigate…") rather than buildable work. Do not score it
  — `:Type: tech-debt` alone is too broad to catch this. Surface it as *needs a
  user decision before it can become a slice*.

Score everything else on four axes:

- **Impact** (×3) — how much shipping this improves the product, developer
  trust, or reduces live risk. If the item carries a `:Priority-signal:` from a
  prior review-triage, use it only as a starting anchor, then adjust from the
  item's own prose — this axis never writes back to that field.
- **Thin-slice fit** (×2) — how small and self-contained the work reads; `3` for
  a single well-scoped change, `1` for open-ended or multi-area work. Inverted
  on purpose — palette already favors thin slices.
- **Readiness & leverage** (×2) — is it fully specified and ready to scope now,
  and does shipping it unblock or de-risk other backlog items.
- **Recurrence** (×1) — score `2`/`3` only when the body or title contains an
  explicit resurfacing cue ("again", "still", "outstanding", a second item
  referencing the same fix) — this axis has no date field to read and does not
  estimate age from silence; default `1`.

`Score = Impact×3 + Thin-slice fit×2 + Readiness & leverage×2 + Recurrence×1`
(range 8–24).

- `17–24` → **strong next-slice candidate**
- `11–16` → **candidate**
- `8–10` → **not yet**

Recommend the highest-scoring item(s) to the user as the next slice; this
ranking is conversational and ephemeral — it is never written back into
`backlog.rst`.

### (b) Story-boundary — own story or fold into a sibling

Applies only while drafting stories in loop step 3, before the hand-off gate.
Once a story is approved at hand-off, its `Done when` / `Not this story` are the
approved spec under the Forced Spec/Plan Deviation rule — do not re-run this
rubric to re-litigate an approved story's boundary.

If a candidate chunk is itself an unresolved decision ("pick a library before
this can be scoped") rather than buildable work, it isn't ready to become a
story yet — route it back to the backlog or a spec pull-helper; do not score it.

Score everything else on four axes:

- **Independent verifiability** (×3) — can its `Done when` be checked by a
  non-developer without any sibling chunk also being done.
- **Distinct footprint** (×2) — does it touch files/components no sibling chunk
  touches.
- **AC substance** (×2) — does it carry at least two meaningful `Done when`
  criteria of its own, not one trivial line.
- **Sequencing constraint** (×1) — does it have a hard build-order relationship
  to siblings that a merged story would obscure.

`Score = Independent verifiability×3 + Distinct footprint×2 + AC substance×2 +
Sequencing constraint×1` (range 8–24).

- `17–24` → recommend its own `story-<n>-<slug>.rst`
- `11–16` → borderline — recommend folding into the closest sibling story,
  noting why in that story's body
- `8–10` → recommend folding into a sibling story

If a single chunk's own `Done when` list runs past ~5 distinct criteria, treat
it as two candidate chunks and score each separately rather than recommending
one oversized story.

### (c) Review-triage priority — what signal a new/deferred item carries into the backlog

Applies at loop step 5, when a review surfaces a new or deferred item, before it
is written into `backlog.rst` with user consent. Reuses the blocked /
decision-gate carve-outs from (a) — if either applies, write that label
directly into `:Priority-signal:` instead of scoring.

Score everything else on four axes:

- **Impact if unresolved** (×3) — how much leaving this unaddressed hurts
  correctness, trust, or safety versus being cosmetic.
- **Blocks current commitments** (×2) — does it put at risk the exit criteria
  of the phase that's closing, or a story/phase already in flight.
- **Recurrence** (×2) — is this the first time this class of issue has
  surfaced, or has something like it come up before. Weighted higher than in
  (a) — noticing a repeating problem is what review triage is for.
- **Cost-to-fix** (×1, inverted) — `3` if it reads as cheap to eventually
  resolve, `1` if it reads as open-ended.

`Score = Impact×3 + Blocks current commitments×2 + Recurrence×2 + Cost-to-fix×1`
(range 8–24), same bands as (a): `17–24 high`, `11–16 medium`, `8–10 low`.

Write the resulting label into the new item's `:Priority-signal:` field as it
enters the backlog — Tier-A bookkeeping, like recording a story `done` in
`index.rst`, never a `:Status:` promotion. This rubric is the only one that
writes `:Priority-signal:`; refresh it here at each later review, not from (a).

## Artifact schemas

All schemas use the robust subset above. `<...>` / `[...]` are fill-ins; size each title underline to the real title.

### `_palette/backlog.rst`

```rst
Backlog — <Project>
===================

Core Product Principles
-----------------------

- <durable principle that should outlive any single phase>

Items
-----

Each item uses the shape below. Do not group items into phase sections and do not
use checkboxes here. An item's :Status: moves to ``in-phase-<N>`` only after the
user approves that phase's scope. ``:Priority-signal:`` is optional, set only by
review-triage; it is Tier-A bookkeeping and never moves :Status: on its own.

<Short title>
~~~~~~~~~~~~~

:Type: bug | refinement | feature | tech-debt | test | spec-gap | rule-gap | chore
:Source: Phase <N> | User idea | Review triage
:Status: backlog | in-phase-<N> | resolved
:Priority-signal: high | medium | low | blocked | decision-gate

<What it is; why it matters — one or two lines.>
```

### `_palette/phase-<N>/phase-brief.rst`

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

<Include ONLY if the source explicitly stated a stack / framework / storage / auth
directive. Transcribe verbatim, one per line. Omit this section entirely otherwise;
never invent one.>
```

### `_palette/phase-<N>/stories/story-<n>-<slug>.rst`

Filename `story-<n>-<slug>.rst`; slug lowercase-hyphenated, 2–4 words. Story numbers are the build order.

```rst
STORY-<n>: <Short title>
========================

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

### `_palette/phase-<N>/stories/index.rst`

```rst
Stories — <Project> — Phase <N>
===============================

:1: <title> — story-1-<slug>.rst — pending
:2: <title> — story-2-<slug>.rst — pending
```

The status field (`pending` / `done`) is the single source of story state: select the next pending story from it, and record `done` there on completion (Tier A bookkeeping — recording completion, never reopening scope).

### `_palette/reviews.rst`

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
