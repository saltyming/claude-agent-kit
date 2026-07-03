---
name: palette-spec
description: Pull-only palette helper. Produce a technical spec (_palette/tech-spec.rst) for the current phase when the user wants that depth. Not part of the default palette loop; invoke only when explicitly asked, or when a phase clearly needs stack / data-model / API decisions settled before stories.
---

<!-- slate-agent-kit:common -->
# palette-spec — tech spec (optional, pull-only)

This is an **optional, pull-only** palette helper. It is not part of the default loop (backlog → slice → stories → hand off → review), and must never run unprompted as part of slicing or story generation. Produce `_palette/tech-spec.rst` only when the user asks, or when the current phase clearly needs technical decisions settled before stories can be written. Follow the RST house style (robust subset — no tables) in the always-loaded palette rule (`claude-agent-kit--palette.md`).

Prefix messages with `[palette-spec]:`.

## When to use

The phase introduces new data, new APIs, new external dependencies, or overturns an earlier technical decision — and the user wants that pinned down before stories.

## What it produces

`_palette/tech-spec.rst` — project-wide technical decisions. Written once, extended (not rewritten) on later phases.

## Schema

```rst
Tech Spec — <Project>
=====================

Tech Stack
----------

<specific — e.g. "React Native with Expo (managed), TypeScript", not just "RN">

Libraries & Dependencies
------------------------

<library>
~~~~~~~~~

:Decision: <choice; exact version only when synced from a real manifest/lockfile>
:Why: <reason>
:Install: <command>

Data Model
----------

<Entity>
~~~~~~~~

:Fields: <field: type, field: type, ...>
:Notes: <notes>

Storage Strategy
----------------

<library, location, offline behaviour; schema if SQL>

Key Technical Decisions
-----------------------

- <decision>

Platform Constraints
--------------------

- <constraint>

Cross-Environment Boundaries
----------------------------

<Only if features span app + widget, web + native, etc. Per boundary: what is
shared, what is not, and the implication. Omit the section otherwise.>
```

## Forbidden

- Not part of the default loop — do not run unprompted during slicing or story generation.
- Do not write or fix source code; this skill only writes `_palette/tech-spec.rst`.
- Do not use tables or nested RST directives — follow the robust subset.
