---
name: palette-ui
description: Pull-only palette helper. Writes the product's visual language — colors, typography, spacing, components, screen composition — as a design document, with an optional HTML preview. Use only when the user asks, or when a phase needs a visual language before its deliverables can be written.
---

<!-- slate-agent-kit:common -->
# palette-ui

Sets the visual language the deliverables will reference.

## What it produces

- `design/visual-design.rst`, following the design template: *Purpose and scope* (style, light or dark mode, reference products); *Architecture* with subsections for the color palette (hex values for primary, secondary, accent, background, surface, text, error, success), typography (family and the size and weight of each level), spacing, radius and icons, and screen composition; *Interfaces and dependencies* for the component library. Only component categories the current phase uses are described.
- Optionally `_palette/design-preview.html`, a static page the user opens in a browser to see the palette and components. It is a preview, not a document, and lives internal.

## Where it goes

The document lives where `layout.rst` places its family and follows that family's template (`palette-init/templates/`) in the RST house style; subsections under the template's sections carry the specifics. A maintained document states only what the source implements. Content the source does not implement yet is written as the changeset of the record that decides it (`palette-record`), and the staging copy shows it until the implementation lands.

## Boundaries

- Not part of the default loop: run it only when the user asks, or when a phase needs it before its deliverables can be written, and say so.
- It writes documents, not source code.
