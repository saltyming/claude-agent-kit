<!-- slate-agent-kit:common -->
# palette — Product-Intent Outer Loop

palette is an **outer loop** wrapping the inner loop (Three-Phase Workflow + `built-in task list (TaskCreate/TaskUpdate)` + harness delegation). The inner loop finishes one task and forgets; palette adds the missing layer above it: a durable backlog of intent, a thin-slice step feeding the inner loop one increment at a time, and a review step harvesting each completion back into the backlog. It is a **loop, not a product-planning suite**: durable backlog → thin slice → hand off → review → re-plan. Design artifacts (tech spec, UX flow, design brief, project rules) are **optional, pull-only helpers** — the `palette-*` skills — never part of the default loop.

**Artifact shapes live on disk, not here.** The schemas for every palette artifact, the RST house style, and the three scoring rubrics are scaffolded into `_palette/templates/` by `palette-init`. Read the relevant template BEFORE writing any artifact; score slice/story/triage decisions by `_palette/templates/rubrics.md`. If `_palette/` exists but `templates/` is missing, invoke `palette-init` — it backfills just the templates.

## Engagement — folder-gated (HARD)

The trigger is the presence of `_palette/` in the project. No heuristics.

- **`_palette/` exists → engaged.** On non-trivial work, weave the outer loop in *without being asked*: consult `_palette/backlog.rst` as you plan, update it as work completes.
- **`_palette/` absent → dormant.** Do not read, create, update, or mention `_palette/`. One exception: for genuinely project/roadmap-shaped work (multi-increment, spans sessions, framed as "a project / MVP / phase / milestone / roadmap") you may **offer one line** — "Want me to set up palette for this? Invoke the `palette-init` skill." — and nothing more. Never scaffold on your own; for a bug fix, bounded feature, lone refactor, or failing CI, do not even offer.
- **Opt-in is explicit**: only the user invoking `palette-init` creates `_palette/`. The distinction is multi-*increment*, not multi-file — palette wakes only when product intent and sequencing are part of the task.

### Auto-engagement ≠ auto-authority (HARD)

The folder's presence switches on the *advisory* loop — never execution authority. Consulting backlog/stories informs planning; every file change still passes the approval gate. "The backlog says so" is not permission to edit.

## Authority firewall — Tier A / Tier B (HARD)

INV-AUTH-1's procedure (invariant defined in `CLAUDE.md`):

- **Tier A — everything under `_palette/`** (backlog, phase briefs, stories, index, optional artifacts). **Advisory only**: an intent ledger that *proposes*; it becomes executable only after promotion into a current story/plan, user approval, and `built-in task list (TaskCreate/TaskUpdate)` tracking.
- **Tier B — the execution machinery.** The user's approval at the "Get approval" gate *authorizes*; `built-in task list (TaskCreate/TaskUpdate)` entries *track*. Binding.

**Precedence, highest first:** current-turn user instruction → approved-and-tracked Tier-B scope → story acceptance criteria / "Not this story" → phase brief → backlog (future intent, explicitly *not* current scope).

### Scope invariant — palette never changes delivery scope (HARD)

Both directions. palette **never shrinks or defers the requested scope**: narrowing to a thinner slice happens only when the user explicitly approves the narrower slice AND explicitly acknowledges which named parts move to the backlog ("Approve phase 1?" is not enough; "approve A now, deferring B and C to the backlog?" is). Moving approved acceptance criteria, plan items, or required tests/config/docs into the backlog is forbidden scope reduction — INV-SCOPE-1 / GATE-DEVIATION territory (`claude-agent-kit--task-execution.md`). **Completion-time is the danger zone**: during execution or review you may not reclassify an unmet criterion as "future" without explicit user consent.

### Deviation and ambiguity

No new vocabulary: once a story passes hand-off, its `Done when` + `Not this story` ARE the approved spec GATE-DEVIATION protects. Plain ambiguity follows the Clarification heuristic (ask about WHAT, decide HOW yourself).

## Gate bindings — every gate's palette meaning

- **Approval gate (Tier A → Tier B).** The execution loop's "Get approval" node IS the palette hand-off; nothing is edited before it.
- **GATE-SCOPE-CONFIRM.** Proposing a phase/story scope is a user-facing gate: report the proposed slice → explicit approval → hand off. The backlog proposes; the user authorizes.
- **`built-in task list (TaskCreate/TaskUpdate)` projection.** Tracker entries for a story are the tactical projection of *already-approved* acceptance criteria — a translation, never a second backlog or new scope.
- **GATE-DEVIATION.** A story's `Done when` + `Not this story` are the approved spec; moving unmet criteria into the backlog is not an escape hatch.
- **Delegation gates.** A delegate (native or dispatch) receives the *approved, tracked scope* — never the raw `story-*.rst`; a dispatch spec's `objective`/`acceptance` come from approved criteria.
- **Thin-slicing vs INV-SCOPE-1.** Choosing a thinner slice is a planning-time, user-owned decision made *before* approval. Once approved, full-scope delivery applies to that slice.
- **Git.** `_palette/` is the developer's personal planning record — never auto-committed by the agent. `palette-init` offers a self-contained `_palette/.gitignore`; sharing is the developer's own choice.

## The loop

1. **Backlog** (`_palette/backlog.rst`) — the cross-session ledger of product intent. Advisory (Tier A).
2. **Slice → phase** — with the user, cut the next thin increment into `_palette/phase-<N>/phase-brief.rst`; rank candidates first with the next-slice rubric (`templates/rubrics.md` §a). User-owned scope decision.
3. **Stories** — decompose the phase into `_palette/phase-<N>/stories/story-<n>-<slug>.rst` + `index.rst` with acceptance criteria; recommend boundaries via rubric §b. Advisory.
4. **Hand off** — at the "Get approval" gate; the approved story's criteria feed the inner loop. Nothing is edited before this gate.
5. **Review + re-plan** — on story/phase completion: record what shipped/was learned (`_palette/reviews.rst` at phase close), triage new/deferred items into the backlog **with explicit user consent**, scoring each with rubric §c. Then slice the next increment.

The loops meet at exactly two points: the hand-off gate (story criteria enter Understand → Plan → Execute) and the completion harvest (the verified result returns to review/backlog). Story state lives in `index.rst`'s status field (`pending`/`done` — Tier-A bookkeeping); select the next pending story from it and record `done` there on completion.

## backlog vs native memory — routing

Both persist; keep them distinct. Evolving intent / scope / deferrals → `_palette/backlog.rst`. Stable facts about the user, project, or settled decisions → the harness's native memory surface. Never record the same thing in both.
