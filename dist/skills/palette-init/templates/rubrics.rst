Judgment criteria
=================

Three recurring palette decisions have stated criteria. The agent weighs them
and recommends; the user decides. A recommendation never changes an item's
status on its own.

Next phase
----------

Which approved or proposed backlog items to recommend for the next phase.

- Set aside first: an item blocked by something the user cannot resolve now,
  and an item that is a decision rather than work (it needs the user's answer
  before it can be planned).
- Impact: how much the product, the user's trust or a live risk improves.
- Size: a single well-bounded change is a better next phase than open-ended
  work across many areas.
- Readiness: the item is specified well enough to plan now, and finishing it
  unblocks or de-risks others.
- Recurrence: the item or its kind has come back before.

Deliverable boundaries
----------------------

Whether a piece of a phase is its own deliverable or part of another, decided
while the phase is planned and never on an approved deliverable.

- It can be checked by a person without any sibling finished.
- It touches files or components no sibling touches.
- It has at least two meaningful ``Done when`` outcomes of its own; more than
  about five suggests two deliverables.
- A hard build-order relation to a sibling that merging would hide.

Priority signal
---------------

The ``Priority-signal`` a new or deferred backlog item carries, set when the
item is written with the user's consent.

- Damage if left unresolved: correctness, trust or safety over cosmetics.
- Whether it endangers an active phase's exit criteria or work in flight.
- Whether this kind of problem has surfaced before.
- Cost to fix: cheap before open-ended.

``blocked`` and ``decision-gate`` replace the signal when they apply.
