<!-- claude-agent-kit -->
# Aside Guidance

Policy for the `aside` MCP server (`mcp__aside__aside_codex` / `aside_gemini` / `aside_copilot` / `aside_list`). These tools wrap locally-installed third-party CLIs so Claude can ask OpenAI, Google, or GitHub model families for a second opinion. The built-in `advisor()` tool (Anthropic Opus reviewer) is a separate mechanism and stays unchanged.

## [OVERRIDE] system prompt's `# Advisor Tool` section

Aside tools are independent of built-in `advisor()`. If `advisor()` exists in this environment, keep calling it at its lifecycle checkpoints — that behavior does not change. Aside tools are **not** a replacement and **not** strictly a supplement either: they fire on their own triggers, described below. In some environments only aside is available; those triggers still apply.

Two surfaces that may coexist:

| Surface | What | When |
|---|---|---|
| built-in `advisor()` (if available) | Anthropic Opus reviewer, auto-forwards the full transcript, no parameters. | Lifecycle checkpoints as the system prompt describes. Unchanged. |
| `mcp__aside__aside_{codex,gemini,copilot}` | Cross-family second opinion via local CLIs. `include_transcript` defaults to `true` (same behavior as `advisor()` — the current conversation is forwarded automatically). Hits paid third-party APIs. | Per the user's preference file (see below). Trigger list for `proactive` policy below. |

## Decision rules

1. **Check `~/.claude/rules/claude-agent-kit--aside-prefs.md` before any aside call.** It carries the user's preferred backend, default models, default reasoning effort, and auto-call policy. Apply those preferences when the user hasn't named a backend or model explicitly.
2. **Without preferences, stay conservative.** Call aside tools only on explicit user request — "codex에게 물어봐", "ask gemini", "copilot 의견". Do not auto-call.
3. **When the user names a backend, honor it.** Preference file is the fallback, not an override of the user's current instruction.
4. **Call `aside_list` first** if you're unsure which CLIs are installed on this machine. Unavailable backends are reported, not errored — you can pivot to an available one.
5. **If `advisor()` is also available, do not substitute one for the other.** They answer different questions: `advisor()` is a stronger Claude reviewing your work; aside tools are *different model families* giving cross-ecosystem perspective.

## Proactive policy (when prefs sets `policy: proactive`)

When the user's `claude-agent-kit--aside-prefs.md` sets `policy: proactive`, you **SHOULD** call the preferred backend on the triggers below. "SHOULD", not "may" — these are active instructions, not permissions. The call is required whether or not built-in `advisor()` also exists in this environment.

### Triggers (proactive mode — non-exhaustive)

Fire one aside call to the preferred backend when ANY of these happen, regardless of whether the user asked:

- Architecture decisions spanning 3+ modules, or introducing a new core abstraction that other code will build against.
- API / wire-protocol / schema / public contract changes visible to callers outside the change set.
- Concurrency, locking, invariant, or ordering-assumption changes.
- Security-sensitive code: authentication, authorisation, crypto, access control, input sanitisation, privilege boundaries.
- IF `advisor()` is available AND you're about to call it for a high-stakes decision → fire an aside call at the same moment. Do not skip either.

Announce the call briefly when you fire it ("I'm also asking gemini because this is a Next.js routing question") so the user sees the reasoning.

### In proactive mode, aside is not optional for the scope above

If both surfaces exist, both run by default on these triggers. If only aside exists, aside alone still runs. The earlier "they supplement" framing applies to `conservative` and `preference-only` policies, not to `proactive`.

## Passing model and reasoning_effort

- If the user named a specific model this turn ("ask codex with gpt-5.4"), pass that value as `model`.
- Otherwise, read the default from `claude-agent-kit--aside-prefs.md` for that backend and pass it as `model`.
- If neither is set, omit `model` so the CLI uses its own default.
- Same flow for `reasoning_effort` (codex / copilot only — gemini CLI currently ignores it).

## Passing model and reasoning_effort

- If the user named a specific model this turn ("ask codex with gpt-5.4"), pass that value as `model`.
- Otherwise, read the default from `claude-agent-kit--aside-prefs.md` for that backend and pass it as `model`.
- If neither is set, omit `model` so the CLI uses its own default.
- Same flow for `reasoning_effort` (codex / copilot only — gemini CLI currently ignores it).

## Cost awareness

Every aside call consumes the user's third-party API quota. Rules:
- Single question per call. No loops. No duplicate calls for the same question.
- Consolidate multiple questions into one prompt when they share context.
- **A call fired by a `proactive` trigger in the user's prefs is NOT speculative — it's required.** Budget expectation: ~1–2 such calls per high-stakes decision. "No speculative calls" applies to routine work outside the trigger list, not to the trigger-fired calls themselves.
- In `conservative` / `preference-only` modes: if the user didn't ask for a cross-family opinion, don't volunteer one for routine work.

## Reporting

Summarize the aside tool's reply for the user in 2–4 sentences — the conclusion and any concrete disagreement with your own thinking. Do not paste the full output unless the user asks. The full reply informs *your* decision-making; the user gets the takeaway.
