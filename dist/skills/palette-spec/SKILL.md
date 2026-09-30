---
name: palette-spec
description: Pull-only palette helper. Writes the project's technical contracts — data model, APIs, storage, platform constraints — as spec and design documents, and records the technical choices behind them as ADRs or RFCs. Use only when the user asks, or when a phase needs technical decisions settled before its deliverables can be written.
---

<!-- slate-agent-kit:common -->
# palette-spec

Settles and writes down the technical ground a phase builds on.

## What it produces

- Technical choices (stack, libraries with the reason for each, storage strategy) as ADRs, or an RFC when the choice sets a module boundary or public contract (`palette-record`).
- Contracts (data model entities and fields, APIs, on-disk formats, platform constraints) in `spec/<topic>.rst`.
- How the parts fit (components, ownership, cross-environment boundaries such as app and widget or web and native) in `design/<topic>.rst`.

An exact library version is written only when it comes from a real manifest or lockfile.

## Where it goes

The document lives where `layout.rst` places its family and follows that family's template (`palette-init/templates/`) in the RST house style; subsections under the template's sections carry the specifics. A maintained document states only what the source implements. Content the source does not implement yet is written as the changeset of the record that decides it (`palette-record`), and the staging copy shows it until the implementation lands.

## Boundaries

- Not part of the default loop: run it only when the user asks, or when a phase needs it before its deliverables can be written, and say so.
- It writes documents, not source code.
