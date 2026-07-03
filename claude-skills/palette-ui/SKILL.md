---
name: palette-ui
description: Pull-only palette helper. Produce a visual design brief (_palette/design-brief.rst) and an optional design-preview.html for the current phase when the user wants that depth. Not part of the default palette loop; invoke only when explicitly asked, or when a phase needs a visual language defined before stories.
---

<!-- slate-agent-kit:common -->
# palette-ui — design brief (optional, pull-only)

This is an **optional, pull-only** palette helper. It is not part of the default loop (backlog → slice → stories → hand off → review), and must never run unprompted as part of slicing or story generation. Produce `_palette/design-brief.rst` (and, if useful, `_palette/design-preview.html`) only when the user asks, or when the current phase needs a visual language defined before stories can be written. Follow the RST house style (robust subset — no tables) in the always-loaded palette rule (`claude-agent-kit--palette.md`).

Prefix messages with `[palette-ui]:`.

## When to use

The phase introduces or depends on visual elements, components, or styling the stories will reference, and the user wants the visual language set first.

## What it produces

- `_palette/design-brief.rst` — the visual language (palette, typography, components). Written once, extended (not rewritten) on later phases.
- `_palette/design-preview.html` — optional browser preview mockup. This one stays **HTML**, not RST, because it is a rendered artifact the user opens in a browser.

## Schema (`design-brief.rst`)

```rst
Design Brief — <Project>
========================

Visual Direction
----------------

:Style: <e.g. minimal, playful, editorial>
:Mode: <light | dark | both>
:Reference apps: <apps>

Framework / Component Library
-----------------------------

<name>

Colour Palette
--------------

:Primary: <hex>
:Secondary: <hex>
:Accent: <hex>
:Background: <hex>
:Surface: <hex>
:Text: <hex>
:Error: <hex>
:Success: <hex>

Typography
----------

:Font family: <font>
:H1: <size / weight>
:H2: <size / weight>
:Body: <size / weight>
:Caption: <size / weight>

Spacing, Radius, Icons
----------------------

:Spacing scale: <scale>
:Border radius: <value>
:Icon style: <style>

Screen Composition
------------------

<Screen Name>
~~~~~~~~~~~~~

:Purpose: <purpose>
:Sections: <top to bottom>
:Shared components: <list>
:Layout notes: <notes>

Component Guide
---------------

<Include only component categories that appear in the current phase. For each:
name, the variants/states actually used, and the visual rule. Delete unused
categories rather than leaving placeholders.>
```

## Forbidden

- Not part of the default loop — do not run unprompted during slicing or story generation.
- Do not write or fix source code; this skill writes only `_palette/design-brief.rst` and the optional `_palette/design-preview.html`.
- Do not use tables or nested RST directives in the brief — follow the robust subset. (The preview stays HTML.)
