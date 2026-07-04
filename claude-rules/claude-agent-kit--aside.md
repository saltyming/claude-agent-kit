<!-- slate-agent-kit:common -->
# Aside Guidance

Policy for the `aside` MCP server (`mcp__aside__aside_codex` / `aside_copilot` / `aside_claude` / `aside_list`; Kimi plugin installs may expose the same tools under a plugin-prefixed MCP name). These tools wrap locally-installed third-party CLIs so the agent can ask OpenAI, GitHub, or Anthropic CLI backends for a second opinion. If the active harness has its own native advisor/reviewer surface, that surface is separate and stays governed by the harness-specific rule.

## Relation to native advisor surfaces

Aside tools are independent of any harness-native advisor. If such a surface exists in the current environment, keep calling it at the lifecycle checkpoints defined by that harness — aside does not replace it. Aside tools are **not** strictly a supplement either: they fire on their own triggers, described below. In some environments only aside is available; those triggers still apply.

**In Claude Code, the native advisor surface is the built-in `advisor()` tool** (an Anthropic Opus reviewer that auto-forwards the full transcript, no parameters). Claude-specific bindings, replacing the generic wording elsewhere in this file where they conflict:

- Every "native advisor" mention below means `advisor()`. Keep calling it at its lifecycle checkpoints as the system prompt describes — unchanged.
- **The no-concurrency hazard is concrete here:** the Claude Code harness currently breaks `advisor()` when an aside tool call is running at the same time (aside's stdio transport interferes with `advisor()`'s transcript forwarding). Sequential-in-one-turn is fine; same-block or overlapping is not.
- **`advisor()` receives the full, unredacted transcript** including tool inputs and outputs — aside receives the redacted form. When the same session goes to both, they see fundamentally different things; the aside-first pairing remains meaningful because `advisor()` later sees the aside exchange too.

Two surfaces that may coexist:

| Surface | What | When |
|---|---|---|
| harness-native advisor/reviewer (if available) | Native second-review surface. Transcript behavior and parameters are harness-specific. | Lifecycle checkpoints as that harness describes. Unchanged. |
| `mcp__aside__aside_{codex,copilot,claude}` | Second opinion via local CLIs. `include_transcript` defaults to `true` — the current conversation is forwarded automatically, **but in redacted form** (text passes through verbatim; `tool_use` / `tool_result` / `thinking` blocks are replaced with placeholders — see **Transcript redaction** below). All backends run read-only with file-read tools available, so they can inspect files the caller names by path — see **Backend capabilities** below. Hits paid third-party API quota. In Claude Code, `aside_claude` is a separate local Claude CLI advisor, not a cross-family opinion. | Per the user's preference file (see below). Trigger list for `proactive` policy below. |

**Relation to `dispatch`.** `aside` *asks* (read-only); `dispatch` *does* (write-capable, async). The full aside↔dispatch↔native-delegate taxonomy is stated once in `claude-agent-kit--parallel-work.md` — don't conflate the surfaces.

## Decision rules

1. **Check `$HOME/.claude/rules/claude-agent-kit--aside-prefs.md` before any aside call.** It carries the user's preferred backend, default models, default reasoning effort, and auto-call policy. Apply those preferences when the user hasn't named a backend or model explicitly.
2. **Without preferences, stay conservative.** Call aside tools only on explicit user request — "codex에게 물어봐", "ask copilot", "copilot 의견". Do not auto-call.
3. **When the user names a backend, honor it.** Preference file is the fallback, not an override of the user's current instruction.
4. **Call `aside_list` first** if you're unsure which CLIs are installed on this machine. Unavailable backends are reported, not errored — you can pivot to an available one.
5. **If a native advisor is also available, do not substitute one for the other.** They answer different questions: native advisors review within the harness's own review channel; aside tools are *different model families* giving cross-ecosystem perspective.
6. **A current-turn explicit user instruction scoping the consultation to one surface overrides Decision rule 5 and the Proactive policy below — in both directions.** It applies *only* when the user's latest message directly names a surface/backend **and** uses exclusivity or prohibition language — e.g. "only aside", "only the native advisor", "ask codex, not the native advisor", "skip/no/don't call aside", "don't pair them", "use both", "use neither". It does **not** apply to: merely naming one backend ("ask codex"), asking for "a second opinion", cost / privacy / risk concerns the agent infers on its own, or the agent's own reassessment of how high-stakes the work is. When it applies, honor it and do **not** fire the other surface. When the scoping is ambiguous, ask or follow prefs — never treat ambiguity as license to skip a required proactive pair. (Self-generated doubt is governed by the Proactive policy's re-audit ban, not by this rule.)
   - Directional / boundary cases: **only aside** → fire aside, do not call the native advisor. **only native advisor** → call the native advisor, do not fire aside ("I was about to call the native advisor" is not a reason to fire aside against an explicit instruction not to). **use both** → run both, sequenced aside-first (the no-concurrency rule still holds). **use neither** → run neither for that decision. **"get a second opinion"** with no surface named → not exclusive; pick per prefs. **"ask codex"** without only/not/skip → names a backend but does not suppress native-advisor pairing if it is independently triggered.

## Proactive policy (when prefs sets `policy: proactive`)

When the user's `claude-agent-kit--aside-prefs.md` sets `policy: proactive`, you **SHOULD** call the preferred backend on the triggers below. "SHOULD", not "may" — these are active instructions, not permissions. The call is required whether or not a native advisor also exists in this environment (unless the user explicitly scoped the request to one surface — Decision rule 6).

### Triggers (proactive mode — non-exhaustive)

Fire one aside call to the preferred backend when ANY of these happen, regardless of whether the user asked (unless the user explicitly scoped the work to one surface — Decision rule 6):

- Architecture decisions spanning 3+ modules, or introducing a new core abstraction that other code will build against.
- API / wire-protocol / schema / public contract changes visible to callers outside the change set.
- Concurrency, locking, invariant, or ordering-assumption changes.
- Security-sensitive code: authentication, authorisation, crypto, access control, input sanitisation, privilege boundaries.
- **Whenever you are about to call a native advisor — *and the user has not scoped the work to one surface (Decision rule 6)* — fire the aside call FIRST, before the native advisor runs, not alongside it.** The act of *proactively* deciding to call a native advisor is itself the signal that you want a second opinion — pairing is the whole point; but an explicit current-turn instruction to use only one surface overrides this in both directions (Decision rule 6).

  **HARD RULE — no concurrency.** Do NOT call aside and a native advisor **concurrently** — not in the same tool-use block, and not while an aside call is still running. Some harnesses multiplex stdio/MCP traffic in ways that can interfere with native transcript forwarding when another MCP call is active. The constraint is **non-concurrency, not a turn boundary**: the two MAY run **sequentially within the same turn** — fire aside, await its full reply, then call the native advisor in a later tool-use block, with no need to yield to the user between them.

  **Required sequence:**
    1. Fire the aside call and wait for its reply.
    2. Summarise the reply for the user per the Reporting section below.
    3. Once the aside call has fully returned, call the native advisor — a later tool-use block in the **same turn** is fine (no need to wait for a subsequent response or yield to the user); never in the same block, and never while aside is still running.

  **Do NOT re-audit the decision to pair.** Once you've decided to call the native advisor, aside fires — full stop. The other triggers above (architecture / API / concurrency / security) gate the *other* proactive calls, not this one. *"I decided this isn't really high-stakes after all"* is a failure mode, not caution.

  **Explicit user scoping overrides this pair — see Decision rule 6.** The re-audit ban directly above targets *your own* second-guessing; it does **not** override a direct, current-turn user instruction. If the user this turn scoped the work to one surface ("only aside" / "only native advisor" / "skip the aside call" / "use neither"), honor that per Decision rule 6 and do not fire the other surface.

  **Legitimate reasons to skip aside here:** the user's prefs set `policy: conservative` or `policy: preference-only`, OR no aside backend is installed on this machine (`aside_list` reports all unavailable), OR the user this turn explicitly scoped the work to native-advisor only / told you to skip the aside call (Decision rule 6 — explicit scoping overrides the pair in both directions).

Announce the call briefly when you fire it ("I'm also asking codex because this is a Next.js routing question") so the user sees the reasoning.

### In proactive mode, aside is not optional for the scope above

If both surfaces exist, both run by default on these triggers, unless the user explicitly scoped the work to one surface (Decision rule 6). If only aside exists, aside alone still runs. The earlier "they supplement" framing applies to `conservative` and `preference-only` policies, not to `proactive`.

## Passing model and reasoning_effort

- If the user named a specific model this turn ("ask codex with gpt-5.4", "ask claude with sonnet"), pass that value as `model`.
- Otherwise, read the default from `claude-agent-kit--aside-prefs.md` for that backend and pass it as `model`.
- If neither is set, omit `model` so the CLI uses its own default.
- Same flow for `reasoning_effort` (codex, copilot, and claude accept it; claude also accepts `max`).
- If the user named a fallback chain this turn, or `claude-agent-kit--aside-prefs.md` sets a default model fallback chain for the backend in play (a comma-separated list), split it on commas, trim whitespace, and pass it as `model_fallback`. A transient failure (rate limit, quota, model unavailable, auth/permission) automatically retries against the next entry; the response notes when a fallback model answered instead of the first one tried.

## Backend capabilities

The aside MCP server spawns each CLI in the active harness session's cwd with a read-only configuration. **Every backend can read files and grep the workspace itself** — the agent does not need to paste file contents in to let the backend "see" code. Capability matrix:

| Backend | CLI flags | Read files | Grep | Web fetch | Write / exec | Notes |
|---|---|---|---|---|---|---|
| `aside_codex` | `-s read-only -a never` | ✅ | ✅ | (via sandbox-permitted tools) | ❌ | `-s read-only` blocks writes and shell side effects but permits reads. Reads are scoped by the codex sandbox. |
| `aside_copilot` | `--allow-all-tools --available-tools=view,rg,glob,web_fetch` | ✅ (`view`) | ✅ (`rg`) | ✅ (`web_fetch`) | ❌ | Whitelist is narrow and intentional — `bash` / `write_bash` / `read_bash` / `task` / `skill` / `sql` / `store_memory` / `report_intent` are excluded so copilot cannot exec shells or mutate state. aside is a consultation surface, not a delegate. |
| `aside_claude` | `-p --safe-mode --no-session-persistence --permission-mode plan --tools Read,Grep,Glob,WebFetch` | ✅ (`Read`) | ✅ (`Grep`, `Glob`) | ✅ (`WebFetch`) | ❌ | Safe mode disables project customizations/hooks/MCP servers, no session is persisted, and plan permission mode plus the tool list keeps the backend read-only. |

### Implication: prefer file paths over embedded excerpts

When your question is about code that lives on disk in this workspace, **pass the file path** (absolute, or relative to the spawn cwd) in the `question` or `context` parameter and let the backend open it. This is cheaper (no transcript bytes), avoids the 100 KB transcript cap, and lets the backend pull in related files it decides it needs without a second round-trip.

Embed an excerpt in `context` only when:
1. **You want to focus the backend on a specific line range** and not let it wander the rest of the file — pass the excerpt with file:line anchors.
2. **The data is not on disk** — transient tool output like command stdout, API response bodies, or other in-memory state.
3. **The backend would not be able to locate the path on its own** — e.g., a file outside the spawn cwd that a backend's sandbox might refuse.

Do not paste whole files into `context` when a path reference would do. Agents were historically doing this because the old rule read "MUST embed the relevant excerpt"; that rule is now narrowed to the three cases above.

## Transcript redaction — aside differs from native advisors

`include_transcript=true` forwards a tail of the invoking harness's own session log — aside reads it **natively per harness** (Claude Code project transcripts, Codex rollouts excluding headless `codex exec` children, Kimi `wire.jsonl`; selection via `ASIDE_HARNESS`, set at registration) — but the renderer redacts tool-related content before it reaches the third-party CLI, identically across harnesses:

| Content block | What the aside backend actually sees |
|---|---|
| `text` (user / assistant message body) | Original text, verbatim |
| `tool_use` (you called a tool) | `[tool_use: <tool_name>]` — name only. **Arguments / inputs are stripped.** |
| `tool_result` (the tool's output) | `[tool_result]` — placeholder. **The result body is not forwarded.** |
| `thinking` | `[thinking]` — content stripped (no CoT leak to third-party backends). |

This redaction is intentional: tool results routinely contain file contents, grep output, command stdout, API responses, and secrets. Forwarding them raw to OpenAI / Google / GitHub CLIs crosses a trust boundary, and a single large tool output would blow the 100 KB transcript budget anyway.

**Native advisors may be different.** Some harness-native reviewers receive a fuller transcript, including tool inputs and outputs; others may not. When the same session is sent to a native advisor and aside, **assume they may see fundamentally different things**. Do not assume an aside backend has any of the substance your tool calls produced just because the transcript was "forwarded."

### HARD RULE: hand the backend file paths; embed only when necessary

If your question depends on **code that is on disk in this workspace**, the default is: **name the file path(s) in `question` or `context` and instruct the backend to read them itself.** Every backend can read (see **Backend capabilities** above). Do not paste file contents in unless one of the three exceptions below applies.

- **`question`** — the actual ask (required). Include file paths and line anchors here when the question is scoped to a specific region.
- **`context`** — optional framing. Either (a) a list of paths the backend should read, with a sentence of orientation each; or (b) a focused excerpt when an exception below applies.

When to embed an excerpt rather than a path:
1. **Line-range focus.** You want the backend to examine a specific function / block, not wander the whole file. Paste the region with file:line anchors and tell the backend it can read the rest of the file if it needs to.
2. **Off-disk data.** Command stdout, API response body, or other in-memory state. The backend cannot read this; the transcript redacts it. Embed it.
3. **Out-of-workspace path.** A backend's sandbox may refuse a path outside the spawn cwd; if the file is outside (e.g., `/tmp`), embed the excerpt. Codex and copilot reads are scoped by their sandboxes and may fail on out-of-scope paths.

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

- The aside-first-then-native-advisor sequencing rule (above) still holds. A native advisor may see a different transcript when it runs later, so the pairing remains meaningful.
- `include_transcript=false` is still the right choice for fully decontextualised questions ("what is 2+2", "give me idiomatic Rust for X"). Do not pass a redacted transcript when the question doesn't need session context at all — it only wastes tokens.

## Cost awareness

Every aside call consumes the user's third-party API quota. Rules:
- Single question per call. No loops. No duplicate calls for the same question.
- **A `model_fallback` retry is not the loop this rule forbids.** When `model_fallback` is set — via the tool param or a default configured in `claude-agent-kit--aside-prefs.md` — a same-question retry against the next model in that chain, fired automatically by the aside server itself because the prior model failed with a transient backend error (rate limit, quota, model unavailable, auth/permission), is one logical call that happens to make more than one subprocess attempt under the hood — not a second question, and not a speculative duplicate call. "No loops. No duplicate calls for the same question" targets *you* re-asking the same or a rephrased question out of hedging or uncertainty; it does not forbid the server's own internal retry for the SAME question when the prior model could not answer at all. Do not manually re-invoke `aside_codex` / `aside_copilot` / `aside_claude` yourself to simulate this — configure `model_fallback` (or the prefs default) once and let the server's retry handle it. A manual re-call for the same question after a failure IS the loop this rule forbids; configuring `model_fallback` up front is not.
- Consolidate multiple questions into one prompt when they share context.
- **A call fired by a `proactive` trigger in the user's prefs is NOT speculative — it's required (unless the user explicitly scoped the work to one surface — Decision rule 6).** Budget expectation: ~1–2 such calls per advisor-paired decision or other triggered scope. "No speculative calls" applies to routine work outside the trigger list, not to the trigger-fired calls themselves.
- In `conservative` / `preference-only` modes: if the user didn't ask for a cross-family opinion, don't volunteer one for routine work.

## Reporting

Summarize the aside tool's reply for the user in 2–4 sentences — the conclusion and any concrete disagreement with your own thinking. Do not paste the full output unless the user asks. The full reply informs *your* decision-making; the user gets the takeaway.
