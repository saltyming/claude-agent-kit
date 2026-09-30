---
name: palette-rules
description: Pull-only palette helper. Writes the conventions every deliverable of the project follows — shared components, coding patterns, architecture boundaries, accessibility baseline — as a spec document. Use only when the user asks, or when a phase needs shared conventions before its deliverables can be written.
---

<!-- slate-agent-kit:common -->
# palette-rules

Writes down the implementation conventions of the project being built; these are the project's own rules, distinct from this kit's rules.

## What it produces

`spec/conventions.rst`, following the spec template, with the conventions under *Contract*:

- shared components that need a rule: what each is, when it must be used, required variants and accessibility semantics;
- coding patterns needed now: state and data-fetching boundaries, theme and token handling, error and form handling;
- architecture: entry-point boundaries, client and server or native and web separation, where each kind of module lives;
- accessibility: the target level (for example WCAG 2.1 AA) and interaction basics (touch targets, visible focus, contrast).

*Conformance* says how a reviewer checks a deliverable against them. Most useful once the stack is known.

## Where it goes

The document lives where `layout.rst` places its family and follows that family's template (`palette-init/templates/`) in the RST house style; subsections under the template's sections carry the specifics. A maintained document states only what the source implements. Content the source does not implement yet is written as the changeset of the record that decides it (`palette-record`), and the staging copy shows it until the implementation lands.

## Boundaries

- Not part of the default loop: run it only when the user asks, or when a phase needs it before its deliverables can be written, and say so.
- It writes documents, not source code.
