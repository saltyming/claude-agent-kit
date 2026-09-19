<!-- slate-agent-kit:common -->
# palette: Product-Intent Outer Loop

palette is an outer loop around the execution loop. The execution loop finishes one task and keeps nothing; palette adds a durable backlog of intent, a step that cuts the next thin slice and hands it to the execution loop, and a review step that feeds each completion back into the backlog. It is a loop, not a product-planning suite. Design artifacts (tech spec, UX flow, design brief, project rules) are optional helpers produced by the `palette-*` skills when the user asks, and are not part of the default loop.

The shape of every palette artifact, the RST house style, and the three scoring rubrics live on disk in `_palette/templates/`, scaffolded by `palette-init`. Read the relevant template before writing an artifact, and score slice, story, and triage decisions with `_palette/templates/rubrics.md`. If `_palette/` exists but `templates/` is missing, invoke `palette-init`, which backfills the templates only.

## Engagement

The only trigger is whether the project contains `_palette/`. Do not use heuristics.

- **`_palette/` exists: engaged.** On non-trivial work, use the outer loop without being asked: consult `_palette/backlog.rst` as you plan and update it as work completes.
- **`_palette/` absent: dormant.** Do not read, create, update, or mention `_palette/`, and do not scaffold it yourself. Only the user invoking `palette-init` creates it. For work that is shaped like a project or roadmap (several increments, spanning sessions, framed as "a project", "MVP", "phase", "milestone", or "roadmap") you may offer one line, "Want me to set up palette for this? Invoke the `palette-init` skill.", and nothing more. For a bug fix, a bounded feature, a lone refactor, or failing CI, do not offer. The distinction is several increments, not several files.

The folder switches on the advisory loop only. It grants no authority to execute: every file change still passes the approval step (INV-AUTH-1).

## Authority: Tier A and Tier B

- **Tier A** is everything under `_palette/`: backlog, phase briefs, stories, index, optional artifacts. It proposes. An item becomes executable only after it is promoted into a current story or plan and the user approves it.
- **Tier B** is the user's approval at the execution loop's approval step. It authorizes.

Precedence is INV-AUTH-1's. A story's "Not this story" ranks with its acceptance criteria, and the backlog is future intent, not current scope.

palette does not change delivery scope in either direction. Do not shrink or defer the requested scope through it. Narrowing to a thinner slice needs the user to approve the narrower slice and to acknowledge which named parts move to the backlog: "Approve phase 1?" is not enough, and "approve A now, deferring B and C to the backlog?" is. Do not move approved acceptance criteria, plan items, or required tests, config, or docs into the backlog; that is scope reduction (INV-SCOPE-1). The risk is highest at completion time: during execution or review, do not reclassify an unmet criterion as future work without the user's explicit consent.

## What each gate means under palette

- **Approval step (Tier A to Tier B).** The execution loop's approval step is the palette hand-off. Nothing is edited before it.
- **GATE-SCOPE-CONFIRM.** Proposing a phase or story scope is a checkpoint for the user: report the proposed slice, get explicit approval, hand off.
- **GATE-DEVIATION.** Once a story passes hand-off, its `Done when` and `Not this story` are the approved spec. Moving unmet criteria into the backlog is not a way around the gate. Plain ambiguity follows the clarification heuristic in `CLAUDE.md`.
- **Delegation gates.** A delegate, native or dispatch, receives the approved scope and not the raw `story-*.rst`.
- **Thin slicing.** Choosing a thinner slice is a planning-time decision that belongs to the user and happens before approval. After approval, INV-SCOPE-1 applies to that slice.
- **Git.** `_palette/` is the developer's personal planning record. Do not commit it on your own. `palette-init` offers a self-contained `_palette/.gitignore`; sharing the folder is the developer's choice.

## The loop

1. **Backlog** (`_palette/backlog.rst`): the cross-session ledger of product intent.
2. **Slice into a phase.** With the user, cut the next thin increment into `_palette/phase-<N>/phase-brief.rst`. Rank candidates first with the next-slice rubric (`templates/rubrics.md` §a). The user owns this decision.
3. **Stories.** Decompose the phase into `_palette/phase-<N>/stories/story-<n>-<slug>.rst` plus `index.rst`, with acceptance criteria. Recommend boundaries with rubric §b.
4. **Hand off** at the approval step. The approved story's criteria go into the execution loop.
5. **Review and re-plan.** When a story or phase completes, record what shipped and what was learned (`_palette/reviews.rst` at phase close), and triage new or deferred items into the backlog with the user's explicit consent, scoring each with rubric §c. Then slice the next increment.

The two loops meet at two points: the hand-off, where story criteria enter the execution loop, and the completion harvest, where the verified result returns to review and backlog. Story state lives in the status field of `index.rst` (`pending` or `done`). Select the next pending story from it and record `done` there on completion.

## Backlog or native memory

Both persist, so keep them distinct. Evolving intent, scope, and deferrals go to `_palette/backlog.rst`. Stable facts about the user, the project, or settled decisions go to the harness's native memory. Do not record the same thing in both.
