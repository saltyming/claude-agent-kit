<!-- claude-agent-kit -->
# Claude Surface Rules

The Claude Code-specific overlay. The shared Slate rules define the behavior (the articles in `CLAUDE.md`); this file covers what differs in Claude Code: how the rules load, and the harness defaults the user's preferences settle.

## Loading Model

- User-scope instructions live at `$HOME/.claude/CLAUDE.md`; the rule files, including the preference files, live in `$HOME/.claude/rules/` and load with it into every session. The user edits a preference file without reinstalling.
- Skills live under `$HOME/.claude/skills`. Read a selected skill's `SKILL.md` completely before acting on it.

## Standing Instruction

This manual is the user's standing instruction. Where the harness's defaults leave a choice to the user, the user has made it here, and the choice applies; it changes nothing the harness reserves to itself: permissions, safety, how a tool is operated. The bindings below are re-checked at each version bump.

- **Memory.** The harness invites saving corrections and confirmed approaches; § 21 narrows that to facts with no other home, and a rule-shaped correction is proposed as rule text.
- **Minimalism** governs what is added unasked; it does not suppress mentioning adjacent problems, shrink the scope (§ 3) or lower the envelope (§ 9). Cost cautions concern models, delegates and quota, not session length (§ 20).
- **Output styles.** A style's insight or explanation blocks describe code; they do not appraise the agent's own work (§ 19). A style that opens with the result applies to facts and results; a judgment still carries its grounds (§ 19).
- **Subagents.** The harness's guidance to use subagents proactively is read through § 7 and the subagent level: a subagent is worth starting when it can change the outcome at a proportionate cost. `Workflow` runs only when the user opts in for the current turn (`claude-agent-kit--delegation.md`).
