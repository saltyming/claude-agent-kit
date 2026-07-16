<!-- slate-agent-kit:common -->
# Aside Guidance

Policy for the `aside` MCP server (`mcp__aside__aside_codex` / `aside_copilot` / `aside_claude` / `aside_list`; Kimi plugin installs may expose plugin-prefixed names): read-only second opinions from another model family via locally-installed CLIs. Operational mechanics — backend capabilities, transcript redaction detail, path-vs-excerpt technique — live in the aside server's own instructions and tool descriptions; this file owns only *when and how to decide to call*.

**Relation to `dispatch`.** `aside` *asks* (read-only); `dispatch` *does* (write-capable, async). The full taxonomy is stated once in `claude-agent-kit--parallel-work.md`.

## Relation to native advisor surfaces

Aside is independent of any harness-native advisor/reviewer: if one exists, keep calling it at its own lifecycle checkpoints — aside neither replaces nor merely supplements it; aside fires on its own triggers below. Native advisors and aside may also see **different transcripts** (aside's is redacted: text passes through; tool inputs/outputs and thinking are stripped) — never assume an aside backend saw the substance of your tool calls.

**In Claude Code, the native advisor surface is the built-in `advisor()` tool** (an Anthropic Opus reviewer that auto-forwards the full transcript, no parameters). Claude-specific bindings, replacing the generic wording elsewhere in this file where they conflict:

- Every "native advisor" mention below means `advisor()`. Where the running harness exposes it, keep calling it at its lifecycle checkpoints as the system prompt describes; where it is absent (not every Claude Code surface ships it), the aside triggers stand alone.
- **The no-concurrency hazard is concrete here:** the Claude Code harness currently breaks `advisor()` when an aside tool call is running at the same time (aside's stdio transport interferes with `advisor()`'s transcript forwarding). Sequential-in-one-turn is fine; same-block or overlapping is not.
- **`advisor()` receives the full, unredacted transcript** including tool inputs and outputs — aside receives the redacted form. When the same session goes to both, they see fundamentally different things; the aside-first pairing remains meaningful because `advisor()` later sees the aside exchange too.

## Decision rules

1. **Check `$HOME/.claude/rules/claude-agent-kit--aside-prefs.md` before any aside call** — preferred backend, default model / reasoning effort / fallback chain, auto-call policy. Prefs are the fallback; a user's current-turn instruction always outranks them.
2. **Without a prefs file, stay conservative**: call aside only on explicit user request; never auto-call.
3. **Call `aside_list` first** when unsure which CLIs are installed; unavailable backends are reported, not errored — pivot to an available one.
4. **Never substitute one surface for the other**: native advisors review within the harness's channel; aside gives a *different model family's* perspective.
5. **Explicit current-turn scoping overrides everything else — in both directions.** It applies only when the user's latest message names a surface/backend AND uses exclusivity or prohibition language ("only aside", "only the native advisor", "skip the aside call", "use both", "use neither"). Merely naming a backend ("ask codex"), asking for "a second opinion", or your own inferred cost/privacy/stakes concerns do NOT count. When scoping applies, honor it and do not fire the other surface; when ambiguous, ask or follow prefs — ambiguity is never license to skip a required proactive pair.

## Proactive policy (prefs `policy: proactive`)

You **SHOULD** call the preferred backend — active instruction, not permission — when ANY of these happen, whether or not the user asked (unless Decision rule 5 scoping applies):

- Architecture decisions spanning 3+ modules, or a new core abstraction other code will build against.
- API / wire-protocol / schema / public-contract changes visible outside the change set.
- Concurrency, locking, invariant, or ordering-assumption changes.
- Security-sensitive code: authn/authz, crypto, access control, input sanitisation, privilege boundaries.
- **You are about to call a native advisor** → fire aside FIRST. Proactively deciding to consult a native advisor is itself the signal that a second opinion is wanted — pairing is the point.

**HARD RULE — no concurrency with a native advisor.** Never run aside and a native advisor at the same time (same tool-use block, or while an aside call is still in flight) — stdio/MCP multiplexing can corrupt native transcript forwarding. Sequential in ONE turn is fine and expected: fire aside → await the full reply → summarise it → then call the native advisor in a later block, no user turn needed between them.

**Do NOT re-audit the decision to pair.** Once you decided to call the native advisor, aside fires — "I decided this isn't really high-stakes after all" is the failure mode this line exists to stop. The only legitimate skips: prefs set `conservative`/`preference-only`, no backend installed (`aside_list` all unavailable), or explicit user scoping per Decision rule 5. Announce each proactive call in one line so the user sees the reasoning.

## Making the call

- `model` / `reasoning_effort` / `model_fallback`: user's current-turn choice → prefs default → omit (CLI default). A prefs fallback chain (comma-separated) is split, trimmed, and passed as `model_fallback` — the server retries transient failures itself; never re-invoke manually to simulate a retry (that IS the duplicate-call the cost rule forbids; the server's own fallback retry is one logical call).
- Backends read files themselves (all run read-only with file tools; recursion into MCP is structurally impossible): **pass file paths, not pasted contents**. Embed an excerpt only for a specific line-range focus, off-disk data (command output, API bodies), or paths outside the spawn cwd. `include_transcript=false` for questions that need no session context.
- **Frame the question as an assessment, not a confirmation.** State your own diagnosis/fix as a hypothesis to check ("tell me if that's wrong and why"), never as settled fact — anchoring bites hardest exactly when you are most confident.

## Cost

Every call burns the user's third-party quota: one question per call, no loops, no rephrased re-asks; consolidate related questions sharing context into one prompt. A `proactive`-trigger call is required, not speculative (~1–2 per triggered decision); in `conservative`/`preference-only`, don't volunteer cross-family opinions for routine work.

## Reporting

Summarize the reply for the user in 2–4 sentences — the conclusion plus any concrete disagreement with your own thinking. The full reply informs *your* decisions; the user gets the takeaway.
