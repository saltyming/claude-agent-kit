<!-- slate-agent-kit:common -->
# Consultation (aside)

The `aside` MCP server asks another model family for a read-only opinion through a locally installed CLI. How to call it (backends, transcript redaction, passing paths, framing the question, cost per call) is in the server's own instructions and tool descriptions. This file covers when a consultation is worth it and which settings to pass.

## When to consult

A consultation is worth its cost when an opinion from another model family could change a decision that is still open: an architecture or public-contract choice other code will build on, a change to concurrency or invariants, security-sensitive code, or a diagnosis you cannot settle from the evidence. It is not worth it when the user is already reviewing with you and waiting for your own answer, when the decision is made, or when the question is routine. Apply INV-AUTO-1 and the aside level (INV-AUTO-2) in `$HOME/.claude/rules/claude-agent-kit--aside-prefs.md` (it loads with these rules); at `auto`, say in one line which backend you are asking and why.

A current-turn instruction that names a surface ("ask codex", "only the native advisor", "no aside") outranks the level in either direction.

## Native advisor surfaces

aside is independent of any reviewer built into the harness; neither replaces the other. They may see different transcripts: aside receives a redacted one with tool inputs, tool outputs and thinking removed, so do not assume an aside backend saw the substance of your tool calls. Never run aside and a native advisor at the same time (in one tool-use block, or while an aside call is in flight): the transports can corrupt the native advisor's transcript forwarding. One after the other in the same turn is fine.

In Claude Code the native advisor is the built-in `advisor()` tool, which forwards the full, unredacted transcript and takes no parameters; its own instructions decide when it is called. When both are used in one turn, call aside first and wait for its reply, so `advisor()` also sees the aside exchange.

## Settings

- Backend: the prefs backend unless the user names one. When you do not know which CLIs are installed, call `aside_list` first; an unavailable backend is reported, not raised.
- `model`, `reasoning_effort`, `model_fallback`: the user's current-turn choice, then the prefs value, then omit the parameter so the CLI default applies. Split a comma-separated fallback chain, trim it, and pass it as `model_fallback`. The server retries transient failures along the chain itself; do not re-invoke by hand.

## Reporting

Summarize the reply in two to four sentences: the conclusion, and any concrete disagreement with your own thinking.
