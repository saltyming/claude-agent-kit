<!-- claude-agent-kit -->
# Aside Guidance

Policy for the `aside` MCP server (`mcp__aside__aside_codex` / `aside_gemini` / `aside_copilot` / `aside_list`). These tools wrap locally-installed third-party CLIs so Claude can ask OpenAI, Google, or GitHub model families for a second opinion. The built-in `advisor()` tool (Anthropic Opus reviewer) is a separate mechanism and stays unchanged.

## [OVERRIDE] system prompt's `# Advisor Tool` section

Aside tools are independent of built-in `advisor()`. If `advisor()` exists in this environment, keep calling it at its lifecycle checkpoints — that behavior does not change. Aside tools are **not** a replacement and **not** strictly a supplement either: they fire on their own triggers, described below. In some environments only aside is available; those triggers still apply.

Two surfaces that may coexist:

| Surface | What | When |
|---|---|---|
| built-in `advisor()` (if available) | Anthropic Opus reviewer, auto-forwards the full transcript, no parameters. | Lifecycle checkpoints as the system prompt describes. Unchanged. |
| `mcp__aside__aside_{codex,gemini,copilot}` | Cross-family second opinion via local CLIs. `include_transcript` defaults to `true` — the current conversation is forwarded automatically, **but in redacted form** (text passes through verbatim; `tool_use` / `tool_result` / `thinking` blocks are replaced with placeholders — see **Transcript redaction — aside ≠ advisor()** below). Hits paid third-party API quota. | Per the user's preference file (see below). Trigger list for `proactive` policy below. |

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
- **Whenever you are about to call `advisor()`, fire the aside call FIRST — before `advisor()` runs, not alongside it.** The act of deciding to call `advisor()` is itself the signal that you want a second opinion — pairing is the whole point. **HARD RULE: do NOT call aside and `advisor()` in the same turn, in the same tool-use block, or otherwise concurrently.** The Claude Code harness currently breaks `advisor()` when an aside tool call is running at the same time (aside's stdio transport interferes with `advisor()`'s transcript forwarding), so the two must be strictly sequenced. Required sequence: (1) fire the aside call and wait for its reply, (2) summarise the reply for the user per the Reporting section, (3) in a **subsequent response**, call `advisor()`. Do NOT re-audit this specific decision against the other triggers above (architecture / API / concurrency / security) to talk yourself out of the pair; those gate the *other* proactive calls, not this one. "I decided this isn't really high-stakes after all" is a failure mode, not caution. The only legitimate reasons to skip aside here: the user's prefs set `policy: conservative` / `policy: preference-only`, or no aside backend is installed on this machine (`aside_list` reports all unavailable).

Announce the call briefly when you fire it ("I'm also asking gemini because this is a Next.js routing question") so the user sees the reasoning.

### In proactive mode, aside is not optional for the scope above

If both surfaces exist, both run by default on these triggers. If only aside exists, aside alone still runs. The earlier "they supplement" framing applies to `conservative` and `preference-only` policies, not to `proactive`.

## Passing model and reasoning_effort

- If the user named a specific model this turn ("ask codex with gpt-5.4"), pass that value as `model`.
- Otherwise, read the default from `claude-agent-kit--aside-prefs.md` for that backend and pass it as `model`.
- If neither is set, omit `model` so the CLI uses its own default.
- Same flow for `reasoning_effort` (codex / copilot only — gemini CLI currently ignores it).

## Transcript redaction — aside ≠ advisor()

`include_transcript=true` forwards the session's `.jsonl` transcript, but the aside renderer (`mcp-servers/aside/src/transcript.rs::render_content`) redacts tool-related content before it reaches the third-party CLI:

| Content block | What the aside backend actually sees |
|---|---|
| `text` (user / assistant message body) | Original text, verbatim |
| `tool_use` (you called a tool) | `[tool_use: <tool_name>]` — name only. **Arguments / inputs are stripped.** |
| `tool_result` (the tool's output) | `[tool_result]` — placeholder. **The result body is not forwarded.** |
| `thinking` | `[thinking]` — content stripped (no CoT leak to third-party backends). |

This redaction is intentional: tool results routinely contain file contents, grep output, command stdout, API responses, and secrets. Forwarding them raw to OpenAI / Google / GitHub CLIs crosses a trust boundary, and a single large tool output would blow the 100 KB transcript budget anyway.

**Built-in `advisor()` is different.** `advisor()` is an Anthropic-internal reviewer and receives the full transcript including tool inputs and outputs. When the same session is sent to `advisor()` and aside, **they see fundamentally different things**. Do not assume an aside backend has any of the substance your tool calls produced just because the transcript was "forwarded."

### HARD RULE: embed tool-derived context in `question` / `context`

If your question to an aside backend depends on **what a tool produced** — the file you just read, the grep match you located, the command output you saw, the diff you staged, the line range you inspected — you **must** include the relevant excerpt in the `question` or `context` parameter. The transcript alone will not carry it.

- **`question`** — the actual ask (required).
- **`context`** — tool-derived substance the backend needs: file excerpts, relevant line ranges, command output, diff hunks. Keep it scoped to what the question needs; do not dump whole files.

Example — **bad** (question relies on a file the agent already Read):
> `question`: "Is the locking in `acquire_lock` correct?"
> `include_transcript`: `true`, no `context` passed.
>
> The backend sees `[tool_use: Read]` followed by `[tool_result]` and zero bytes of the function. The answer will be hallucinated or refused.

Example — **good**:
> `question`: "Is the locking in `acquire_lock` correct — specifically the ordering between `lock_a` and `lock_b`?"
> `context`: the ~20 lines of `acquire_lock` pasted as a fenced code block, plus the surrounding callers if relevant.

### What this does not change

- The aside-first-then-advisor() sequencing rule (above) still holds. `advisor()` still sees the full transcript when it runs in the subsequent turn, so it will pick up both the aside exchange and the original tool inputs/outputs — the pairing remains meaningful.
- `include_transcript=false` is still the right choice for fully decontextualised questions ("what is 2+2", "give me idiomatic Rust for X"). Do not pass a redacted transcript when the question doesn't need session context at all — it only wastes tokens.

## Cost awareness

Every aside call consumes the user's third-party API quota. Rules:
- Single question per call. No loops. No duplicate calls for the same question.
- Consolidate multiple questions into one prompt when they share context.
- **A call fired by a `proactive` trigger in the user's prefs is NOT speculative — it's required.** Budget expectation: ~1–2 such calls per advisor-paired decision or other triggered scope. "No speculative calls" applies to routine work outside the trigger list, not to the trigger-fired calls themselves.
- In `conservative` / `preference-only` modes: if the user didn't ask for a cross-family opinion, don't volunteer one for routine work.

## Reporting

Summarize the aside tool's reply for the user in 2–4 sentences — the conclusion and any concrete disagreement with your own thinking. Do not paste the full output unless the user asks. The full reply informs *your* decision-making; the user gets the takeaway.
