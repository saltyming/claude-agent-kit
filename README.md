# Claude Agent Manual

A battle-tested `CLAUDE.md` for Claude Code that explicitly overrides system prompt directives causing scope reduction, reasoning suppression, and false completion claims.

## Why This Exists

Claude Code ships with system prompt directives optimized for casual Q&A — not deep engineering work. These directives conflict with what power users need:

| System Prompt Says | What You Actually Need |
|---|---|
| "Be extra concise. Lead with action, not reasoning." | Explain before acting. Show your reasoning. |
| "Only make changes that are directly requested." | Follow the design doc. Implement the full scope. |
| "Do not create files unless absolutely necessary." | Create every file the spec calls for. |
| (no verification required) | Verify before claiming completion. Never fake a green result. |

This manual quotes each problematic directive and provides an explicit `[OVERRIDE]` — giving the model a concrete alternative instead of a vague "do better."

## What's Inside

- **System prompt conflict resolution** — 10+ override points with quoted system prompt text
- **Agent Teams workflow** — self-claim policy, leader intervention patterns, teammate communication triggers, scope reduction prevention
- **Verification teammate pattern** — dedicated build/test/review role with scaling guidance (single verifier vs split build + semantic review)
- **Quality guardrails** — false claims mitigation, comment discipline, verification fallback for untestable code

## Usage

Copy `CLAUDE.md` to your global config:

```bash
cp CLAUDE.md ~/.claude/CLAUDE.md
```

Or use it as a project-level config:

```bash
cp CLAUDE.md .claude/CLAUDE.md
```

Adapt to your needs. The system prompt overrides and Agent Teams sections are the core value — framework conventions and git workflow are examples you should replace with your own.

## Background

This manual was developed over 2 months of building [SaltyOS](https://github.com/SaltyOS/saltyos), a capability-based microkernel written from scratch in Rust. The project runs 6 parallel Claude Code agents for kernel development, userland servers, and cross-architecture porting. Every rule in this document exists because something went wrong without it.

Key references that informed the system prompt overrides:

- [Claude Code isn't "stupid now": it's being system prompted to act like that](https://github.com/anthropics/claude-code/issues/30027)
- [Follow-up: Claude Code's source confirms the system prompt problem](https://github.com/anthropics/claude-code/issues/30027) — leaked `prompts.ts` showing internal (`ant`) vs external prompt differences

## License

This work is licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

You are free to share and adapt this material for any purpose, including commercial, as long as you give appropriate credit.
