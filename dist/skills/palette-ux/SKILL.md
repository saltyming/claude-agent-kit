---
name: palette-ux
description: Pull-only palette helper. Writes the screens, navigation and flow sequence of the product as a design document. Use only when the user asks, or when a phase's screens and navigation need defining before its deliverables can be written.
---

<!-- slate-agent-kit:common -->
# palette-ux

Maps the product's screens and how a person moves between them.

## What it produces

`design/ux-flow.rst`, following the design template:

- *Purpose and scope*: which part of the product the flow covers.
- *Architecture*: the screen list; navigation per screen in plain language (how it is reached — tab, full screen, modal, sheet — and how the person gets back); the flow sequence as numbered steps.
- *Ownership and state*: per screen, what it shows and what the person can do, with the result of each action.
- *Failure and recovery*: empty, error and offline states.
- The remaining sections state `None.` when they do not apply.

Later phases extend it with new or changed screens only.

## Where it goes

The document lives where `layout.rst` places its family and follows that family's template (`palette-init/templates/`) in the RST house style; subsections under the template's sections carry the specifics. A maintained document states only what the source implements. Content the source does not implement yet is written as the changeset of the record that decides it (`palette-record`), and the staging copy shows it until the implementation lands.

## Boundaries

- Not part of the default loop: run it only when the user asks, or when a phase needs it before its deliverables can be written, and say so.
- It writes documents, not source code.
