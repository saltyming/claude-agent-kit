<!-- claude-agent-kit -->
# Aside Guidance

Policy for the `aside` MCP server (`mcp__aside__aside_codex` / `aside_gemini` / `aside_copilot` / `aside_list`). These tools wrap locally-installed third-party CLIs so Claude can ask OpenAI, Google, or GitHub model families for a second opinion. The built-in `advisor()` tool (Anthropic Opus reviewer) is a separate mechanism and stays unchanged.

## [OVERRIDE] system prompt's `# Advisor Tool` section

The system prompt teaches you to call built-in `advisor()` at lifecycle checkpoints — before substantive work, when stuck, before declaring done. Keep doing that. Aside tools **do not replace** built-in `advisor()`; they supplement it with cross-family perspective.

Two surfaces exist:

| Surface | What | When |
|---|---|---|
| built-in `advisor()` | Anthropic Opus reviewer, auto-forwards the full transcript, no parameters. | Lifecycle checkpoints as the system prompt describes. Unchanged. |
| `mcp__aside__aside_{codex,gemini,copilot}` | Cross-family second opinion via local CLIs. `include_transcript` defaults to `true` (same behavior as `advisor()` — the current conversation is forwarded automatically). Hits paid third-party APIs. | Per the user's preference file (see below). Otherwise conservative — only on explicit user request. |

## Decision rules

1. **Built-in `advisor()` behavior is unchanged.** Never skip it to "save cost" by substituting an aside tool. They answer different questions: `advisor()` is a stronger Claude reviewing your work; aside tools are *different model families* giving cross-ecosystem perspective.
2. **Check `~/.claude/rules/claude-agent-kit--aside-prefs.md` before any aside call.** It carries the user's preferred backend, default models, default reasoning effort, and auto-call policy. Apply those preferences when the user hasn't named a backend or model explicitly.
3. **Without preferences, stay conservative.** Call aside tools only on explicit user request — "codex에게 물어봐", "ask gemini", "copilot 의견". Do not auto-call.
4. **When the user names a backend, honor it.** Preference file is the fallback, not an override of the user's current instruction.
5. **Call `aside_list` first** if you're unsure which CLIs are installed on this machine. Unavailable backends are reported, not errored — you can pivot to an available one.

## Passing model and reasoning_effort

- If the user named a specific model this turn ("ask codex with gpt-5.4"), pass that value as `model`.
- Otherwise, read the default from `claude-agent-kit--aside-prefs.md` for that backend and pass it as `model`.
- If neither is set, omit `model` so the CLI uses its own default.
- Same flow for `reasoning_effort` (codex / copilot only — gemini CLI currently ignores it).

## Cost awareness

Every aside call consumes the user's third-party API quota. Rules:
- Single question per call. No speculative calls. No loops.
- Consolidate multiple questions into one prompt when they share context.
- If the user didn't ask for a cross-family opinion, don't volunteer one for routine work.

## Reporting

Summarize the aside tool's reply for the user in 2–4 sentences — the conclusion and any concrete disagreement with your own thinking. Do not paste the full output unless the user asks. The full reply informs *your* decision-making; the user gets the takeaway.
