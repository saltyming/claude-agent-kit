---
name: palette-rules
description: Pull-only palette helper. Produce project rules (_palette/project-rules.rst) — components, patterns, architecture, accessibility — when the user wants project conventions on file. Not part of the default palette loop; invoke only when explicitly asked, or when a phase needs shared conventions defined before stories.
---

<!-- slate-agent-kit:common -->
# palette-rules — project rules (optional, pull-only)

This is an **optional, pull-only** palette helper. It is not part of the default loop (backlog → slice → stories → hand off → review), and must never run unprompted as part of slicing or story generation. Produce `_palette/project-rules.rst` only when the user asks, or when the current phase needs shared conventions defined before stories can be written. Follow the RST house style (robust subset — no tables) in the always-loaded palette rule (`claude-agent-kit--palette.md`).

These are project-level product/architecture conventions for the code being built — distinct from this kit's own operating rules. Most useful once the tech stack is known.

Prefix messages with `[palette-rules]:`.

## When to use

The project needs agreed conventions (component reuse, coding patterns, architecture boundaries, accessibility baseline) recorded so every story follows them.

## What it produces

`_palette/project-rules.rst` — conventions the implementer follows on every story. Extended or amended over time.

## Schema

```rst
Project Rules — <Project>
=========================

The implementer follows these on every story. Extend or amend as needed.

Components
----------

<Shared components that genuinely need a rule: what it is, when it must be used,
required variants / props / accessibility semantics. Do not duplicate the design
brief's component inventory — add only implementation rules it lacks.>

Patterns
--------

<Coding patterns needed now: state / data-fetching boundaries, theme/token
management, error/form handling, when UI should become shared.>

Architecture
------------

<Routing / entrypoint boundaries, client-server or native/web separation,
file-placement rules for services / modules / containers.>

Accessibility
-------------

:Level: <e.g. WCAG 2.1 AA>

- <interaction basics: minimum touch targets, visible focus indicators, contrast>
```

## Forbidden

- Not part of the default loop — do not run unprompted during slicing or story generation.
- Do not write or fix source code; this skill only writes `_palette/project-rules.rst`.
- Do not use tables or nested RST directives — follow the robust subset.
