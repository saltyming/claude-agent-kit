<!-- slate-agent-kit:common -->
# Aside Guidance

Policy for the `aside` MCP server: read-only second opinions from another model family through locally installed CLIs. How to call it (backends, transcript redaction, passing paths, framing the question, cost per call) is in the server's own instructions and tool descriptions. This file covers when to call and which settings to pass. For how aside relates to `dispatch`, see `claude-agent-kit--parallel-work.md`.

## Native advisor surfaces

aside is independent of any reviewer built into the harness. If the harness has one, keep calling it at its own checkpoints; aside does not replace it, and aside fires on the triggers below. The two may see different transcripts: aside receives a redacted one with tool inputs, tool outputs, and thinking removed, so do not assume an aside backend saw the substance of your tool calls.

In Claude Code the native advisor is the built-in `advisor()` tool, which forwards the full transcript and takes no parameters. Some Claude Code surfaces do not ship it; there, the aside triggers stand alone.

- The concurrency hazard below is concrete here: `advisor()` breaks when an aside call is running at the same time, because aside's stdio transport interferes with the transcript forwarding.
- `advisor()` receives the unredacted transcript, including tool inputs and outputs, while aside receives the redacted form. Calling aside first still makes sense, because `advisor()` then sees the aside exchange too.

## Decision rules

1. Read `$HOME/.claude/rules/claude-agent-kit--aside-prefs.md` before any aside call. It gives the preferred backend, the default model, reasoning effort, and fallback chain, and the auto-call policy. A current-turn instruction from the user outranks it.
2. Without a prefs file, call aside only when the user asks. Do not auto-call.
3. When you do not know which CLIs are installed, call `aside_list` first. An unavailable backend is reported, not raised as an error; switch to an available one.
4. Do not substitute one surface for the other. A native advisor reviews inside the harness's channel; aside gives another model family's view.
5. A current-turn scoping instruction from the user overrides everything else, in both directions. It counts as scoping when the user's latest message names a surface or backend and uses exclusive or prohibiting language ("only aside", "only the native advisor", "skip the aside call", "use both", "use neither"). Naming a backend ("ask codex"), asking for "a second opinion", and your own concerns about cost, privacy, or stakes are not scoping. When scoping applies, follow it and do not fire the other surface. When the message is ambiguous, ask or follow prefs. Ambiguity is not a reason to skip a required pair.

## Proactive policy (prefs `policy: proactive`)

Call the preferred backend, whether or not the user asked, when any of these happens (unless rule 5 scoping applies):

- An architecture decision that spans three or more modules, or a new core abstraction other code will build on.
- A change to an API, wire protocol, schema, or public contract that is visible outside the change set.
- A change to concurrency, locking, invariants, or ordering assumptions.
- Security-sensitive code: authentication and authorization, crypto, access control, input sanitization, privilege boundaries.
- You are about to call a native advisor. Call aside first. Deciding to consult a native advisor is itself the sign that a second opinion is wanted.

Do not run aside and a native advisor at the same time, meaning in the same tool-use block or while an aside call is still in flight: stdio/MCP multiplexing can corrupt the native advisor's transcript forwarding. Running them one after the other in a single turn is expected: call aside, wait for the full reply, summarize it, then call the native advisor in a later block. No user turn is needed between them.

Once you have decided to call the native advisor, do not reconsider whether to pair it with aside. "This is not high-stakes after all" is the failure this rule exists to stop. The pair is skipped only when prefs set `conservative` or `preference-only`, when no backend is installed (`aside_list` reports all unavailable), or when the user scoped the call under rule 5. Announce each proactive call in one line so the user sees why it fired. Under `conservative` or `preference-only`, do not volunteer cross-family opinions for routine work.

## Settings

For `model`, `reasoning_effort`, and `model_fallback`, use the user's current-turn choice, then the prefs default, then omit the parameter so the CLI default applies. Split a comma-separated prefs fallback chain, trim it, and pass it as `model_fallback`. The server retries transient failures along that chain on its own. Do not re-invoke by hand to simulate a retry: that is a duplicate call, while the server's own retry counts as one.

## Reporting

Summarize the reply for the user in two to four sentences: the conclusion, and any concrete disagreement with your own thinking.
