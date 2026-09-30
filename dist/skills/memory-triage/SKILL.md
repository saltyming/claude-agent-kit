---
name: memory-triage
description: Review the harness's native memory for a project and propose, memory by memory, whether to keep it, promote it to a rule, revise it, merge it, or delete it, each with a reason. Deletes nothing without the user's choice. Use when the user asks to clean up or review memory, or when a memory is found contradicting the rules or the project's documents.
---

<!-- slate-agent-kit:common -->
# memory-triage

Brings native memory back to what INV-MEM-1 allows: facts with no other home, one per memory, current. Every change is a proposal until the user chooses.

## Where memory lives

- **Claude Code**: `~/.claude/projects/<project slug>/memory/`, one file per memory and an index `MEMORY.md`. The slug is derived from the project's path; list `~/.claude/projects/` and pick the folder that matches the project.
- **Codex**: Memories are generated in the background from past sessions under `~/.codex/memories/`; the agent does not edit them. Propose changes through Codex's own memory controls instead.
- **Kimi Code**: no native memory.

## Read before judging

Read every memory file, and the places a memory may duplicate: this kit's rules, the project's instruction files, and its palette and maintained documents. A memory that names a file, symbol, path or flag is checked against the working tree.

## Classify each memory

- **keep**: true, has no other home, states one rule or fact.
- **promote**: a rule for this project, or one that holds for every project. Propose the exact text and where it goes (the project's instruction file, or this kit); once the user accepts the text there, the memory is deleted.
- **covered**: a rule or document already states it. Name the place and quote the sentence. If you cannot quote it, it is not covered: deleting a memory whose rule exists nowhere else brings the mistake back.
- **revise**: true but mis-shaped (a narrative, appended corrections or dated incident paragraphs, several rules in one). Propose the rewritten text: the rule, then its reason in one or two sentences.
- **merge**: the same trigger as another memory. Propose the merged text and which name survives.
- **delete**: false; stale (what it names no longer exists); a description of code; progress or status; a temporary condition; session context.

## Propose, then apply what the user chooses

Present the proposal grouped by class, one line per memory: the file, the reason, and for promote, revise and merge the proposed text. Wait.

Apply only the chosen items: edit or delete the files, update the index lines, and update every `[[link]]` to a renamed, merged or deleted memory in the same change. Report what changed.
