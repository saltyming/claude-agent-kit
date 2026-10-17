<!-- slate-agent-kit:common -->
# Consultation

`dispatch_consult` asks another model family for a read-only opinion through the `dispatch` MCP server; how to call it is in the server's own instructions, and how to wait for it is in the dispatch rule.

- Worth its cost when another family's view can change an open decision: an architecture or public-contract choice, a change to concurrency or invariants, security-sensitive code, a diagnosis the evidence does not settle. Not when the decision is made, the question is routine, or the user is reviewing with you and waiting for your own answer. The agent puts no decision to the user that a consultation was submitted to inform until its reply is read, or its failure or cancellation reported. Level: `$HOME/.claude/rules/claude-agent-kit--consult-prefs.md` (it loads with these rules); at `auto`, say in one line which backend and why (§ 8).
- A consultation and a harness-native advisor are independent and may see different transcripts (the consultation's is redacted of tool calls); never run both at once, one after the other is fine.
- Backend, `model`, `reasoning_effort`, `model_fallback`: the user's current-turn choice, then the prefs, then omit. Pass the fallback chain split and trimmed; the server retries transient failures itself. Unknown CLIs: `dispatch_backends` first.
- A consultation runs in the background like an execution: submit, wait as the dispatch rule says, read the reply from `dispatch_status`, stop it only with `dispatch_cancel`. A backend that dispatch started may consult and may not submit.
- Report in two to four sentences: the conclusion and any concrete disagreement with your own view. A citation in the reply (a file, a log, a figure) is checked against the primary source before it is relied on; a backend has fabricated evidence before.

In Claude Code the native advisor is the built-in `advisor()` tool, which forwards the full, unredacted transcript and takes no parameters; its own instructions decide when it is called. When both are used in one turn, consult first and read the reply, so `advisor()` also sees the exchange.
