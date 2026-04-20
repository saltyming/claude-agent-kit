<!-- claude-agent-kit -->
# Aside Guidance

Policy for the `aside` MCP server (`mcp__aside__aside_codex` / `aside_gemini` / `aside_copilot` / `aside_list`). These tools wrap locally-installed third-party CLIs so Claude can ask OpenAI, Google, or GitHub model families for a second opinion. The built-in `advisor()` tool (Anthropic Opus reviewer) is a separate mechanism and stays unchanged.

## [OVERRIDE] system prompt's `# Advisor Tool` section

Aside tools are independent of built-in `advisor()`. If `advisor()` exists in this environment, keep calling it at its lifecycle checkpoints — that behavior does not change. Aside tools are **not** a replacement and **not** strictly a supplement either: they fire on their own triggers, described below. In some environments only aside is available; those triggers still apply.

Two surfaces that may coexist:

| Surface | What | When |
|---|---|---|
| built-in `advisor()` (if available) | Anthropic Opus reviewer, auto-forwards the full transcript, no parameters. | Lifecycle checkpoints as the system prompt describes. Unchanged. |
| `mcp__aside__aside_{codex,gemini,copilot}` | Cross-family second opinion via local CLIs. `include_transcript` defaults to `true` — the current conversation is forwarded automatically, **but in redacted form** (text passes through verbatim; `tool_use` / `tool_result` / `thinking` blocks are replaced with placeholders — see **Transcript redaction — aside ≠ advisor()** below). All three backends run read-only with file-read tools available, so they can inspect files the caller names by path — see **Backend capabilities** below. Hits paid third-party API quota. | Per the user's preference file (see below). Trigger list for `proactive` policy below. |

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
- **Whenever you are about to call `advisor()`, fire the aside call FIRST — before `advisor()` runs, not alongside it.** The act of deciding to call `advisor()` is itself the signal that you want a second opinion — pairing is the whole point.

  **HARD RULE — no concurrency.** Do NOT call aside and `advisor()` in the same turn, in the same tool-use block, or otherwise concurrently. The Claude Code harness currently breaks `advisor()` when an aside tool call is running at the same time (aside's stdio transport interferes with `advisor()`'s transcript forwarding), so the two must be strictly sequenced.

  **Required sequence:**
    1. Fire the aside call and wait for its reply.
    2. Summarise the reply for the user per the Reporting section below.
    3. In a **subsequent response**, call `advisor()`.

  **Do NOT re-audit the decision to pair.** Once you've decided to call `advisor()`, aside fires — full stop. The other triggers above (architecture / API / concurrency / security) gate the *other* proactive calls, not this one. *"I decided this isn't really high-stakes after all"* is a failure mode, not caution.

  **Legitimate reasons to skip aside here:** the user's prefs set `policy: conservative` or `policy: preference-only`, OR no aside backend is installed on this machine (`aside_list` reports all unavailable).

Announce the call briefly when you fire it ("I'm also asking gemini because this is a Next.js routing question") so the user sees the reasoning.

### In proactive mode, aside is not optional for the scope above

If both surfaces exist, both run by default on these triggers. If only aside exists, aside alone still runs. The earlier "they supplement" framing applies to `conservative` and `preference-only` policies, not to `proactive`.

## Passing model and reasoning_effort

- If the user named a specific model this turn ("ask codex with gpt-5.4"), pass that value as `model`.
- Otherwise, read the default from `claude-agent-kit--aside-prefs.md` for that backend and pass it as `model`.
- If neither is set, omit `model` so the CLI uses its own default.
- Same flow for `reasoning_effort` (codex / copilot only — gemini CLI currently ignores it).

## Backend capabilities

The aside MCP server spawns each CLI in the Claude Code session's cwd with a read-only configuration. **Every backend can read files and grep the workspace itself** — the agent does not need to paste file contents in to let the backend "see" code. Capability matrix:

| Backend | CLI flags | Read files | Grep | Web fetch | Write / exec | Notes |
|---|---|---|---|---|---|---|
| `aside_codex` | `-s read-only -a never` | ✅ | ✅ | (via sandbox-permitted tools) | ❌ | `-s read-only` blocks writes and shell side effects but permits reads. Reads are scoped by the codex sandbox. |
| `aside_gemini` | `--approval-mode plan` | ✅ | ✅ | ✅ | ❌ | Plan mode: read / grep / web tools available, no edits, no shell exec, no approval prompts. Reads are restricted to the spawn-cwd workspace (attempting a path outside it returns `Path not in workspace`). |
| `aside_copilot` | `--allow-all-tools --available-tools=view,rg,glob,web_fetch` | ✅ (`view`) | ✅ (`rg`) | ✅ (`web_fetch`) | ❌ | Whitelist is narrow and intentional — `bash` / `write_bash` / `read_bash` / `task` / `skill` / `sql` / `store_memory` / `report_intent` are excluded so copilot cannot exec shells or mutate state. aside is a consultation surface, not a delegate. |

### Implication: prefer file paths over embedded excerpts

When your question is about code that lives on disk in this workspace, **pass the file path** (absolute, or relative to the spawn cwd) in the `question` or `context` parameter and let the backend open it. This is cheaper (no transcript bytes), avoids the 100 KB transcript cap, and lets the backend pull in related files it decides it needs without a second round-trip.

Embed an excerpt in `context` only when:
1. **You want to focus the backend on a specific line range** and not let it wander the rest of the file — pass the excerpt with file:line anchors.
2. **The data is not on disk** — transient tool output like command stdout, API response bodies, staged diffs that are in a workslate buffer but not yet applied, in-memory state.
3. **The backend would not be able to locate the path on its own** — e.g., a file outside the spawn cwd that gemini's workspace restriction would refuse.

Do not paste whole files into `context` when a path reference would do. Agents were historically doing this because the old rule read "MUST embed the relevant excerpt"; that rule is now narrowed to the three cases above.

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

### HARD RULE: hand the backend file paths; embed only when necessary

If your question depends on **code that is on disk in this workspace**, the default is: **name the file path(s) in `question` or `context` and instruct the backend to read them itself.** Every backend can read (see **Backend capabilities** above). Do not paste file contents in unless one of the three exceptions below applies.

- **`question`** — the actual ask (required). Include file paths and line anchors here when the question is scoped to a specific region.
- **`context`** — optional framing. Either (a) a list of paths the backend should read, with a sentence of orientation each; or (b) a focused excerpt when an exception below applies.

When to embed an excerpt rather than a path:
1. **Line-range focus.** You want the backend to examine a specific function / block, not wander the whole file. Paste the region with file:line anchors and tell the backend it can read the rest of the file if it needs to.
2. **Off-disk data.** Command stdout, API response body, a staged workslate buffer that is not yet applied, in-memory state. The backend cannot read this; the transcript redacts it. Embed it.
3. **Out-of-workspace path.** Gemini's workspace restriction refuses paths outside the spawn cwd; if the file is outside (e.g., `/tmp`), embed the excerpt. Codex and copilot are less strict but may also fail depending on sandbox.

Example — **bad** (pastes a whole file when a path would do):
> `question`: "Is the locking in `acquire_lock` correct?"
> `context`: 800 lines of `src/sync/lock.rs` pasted verbatim.
>
> Wasteful. The backend can open the file itself.

Example — **good** (path-first):
> `question`: "Read `src/sync/lock.rs` and tell me whether the ordering between `lock_a` and `lock_b` in `acquire_lock` is safe. Check the call sites too."
> `context`: *(empty, or a one-liner: "related: `src/sync/mod.rs` defines the `Lock` type")*
>
> The backend reads the file and decides what else it needs to look at.

Example — **good** (line-range focus):
> `question`: "In `src/sync/lock.rs`, look at `acquire_lock` (lines 142-178). Is the ordering between `lock_a` and `lock_b` correct? The rest of the file is lock plumbing and not relevant to this question."
> `context`: the 36-line excerpt pasted as a fenced code block.

Example — **good** (off-disk data, exception 2):
> `question`: "Is this `cargo build` output consistent with a linker issue, or is it a codegen bug?"
> `context`: the 50-line stderr excerpt pasted as a fenced code block.

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
