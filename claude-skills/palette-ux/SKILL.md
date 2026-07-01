---
name: palette-ux
description: Pull-only palette helper. Produce a UX flow (_palette/ux-flow.rst) — screens, navigation, flow sequence — for the current phase when the user wants that depth. Not part of the default palette loop; invoke only when explicitly asked, or when a phase's screens and navigation need defining before stories.
---

<!-- claude-agent-kit -->
# palette-ux — UX flow (optional, pull-only)

This is an **optional, pull-only** palette helper. It is not part of the default loop (backlog → slice → stories → hand off → review), and must never run unprompted as part of slicing or story generation. Produce `_palette/ux-flow.rst` only when the user asks, or when the current phase's screens and navigation need defining before stories can be written. Follow the RST house style (robust subset — no tables) in the always-loaded palette rule (`claude-agent-kit--palette.md`).

Prefix messages with `[palette-ux]:`.

## When to use

The phase has non-trivial screens, navigation, or an interaction sequence the stories will depend on, and the user wants that mapped first.

## What it produces

`_palette/ux-flow.rst` — project-wide screen and flow map. Written once, extended (not rewritten) on later phases with only new or changed screens.

## Schema

```rst
UX Flow — <Project>
===================

Screen List
-----------

1. <screen>

Navigation
----------

<plain language, per screen: how it is reached (tab / full screen / modal /
bottom sheet) and how the user gets back. e.g. "Tapping a goal on Today opens
Goal Detail full screen; back returns to Today.">

Flow Sequence
-------------

1. <step>

Screen Specs
------------

<Screen Name>
~~~~~~~~~~~~~

:Shows: <what is on screen>
:User can: <actions>
:On action: <result>
:Constraints: <constraints>
```

## Forbidden

- Not part of the default loop — do not run unprompted during slicing or story generation.
- Do not write or fix source code; this skill only writes `_palette/ux-flow.rst`.
- Do not use tables or nested RST directives — follow the robust subset.
