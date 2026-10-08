<!-- slate-agent-kit:common -->
# Work Leaving the Session

Subagents, consultation (`claude-agent-kit--aside.md`) and dispatch (`claude-agent-kit--dispatch.md`), each judged for value (§ 7) and used at its level (§ 8).

- Subagents are the harness's delegates: read-only ones inspect, search or summarize; write-capable ones edit; an unknown kind is write-capable. Consultation asks for an opinion and writes nothing. Dispatch hands a self-contained, write-capable step to an external agent that runs asynchronously. Skills, commands and workflows bypass no article. When the harness lacks a mechanism safe delegation needs, say so; do not improvise one.
- A delegate saves the leader's context and costs the user models, quota and time, returning a result without its reasoning. It helps when work splits into independent parts with a stable shared contract, or when one bounded lookup would flood the context. Work stays in-session when sequential, tightly coupled, small, assigned to the leader by a document, or when the user is waiting for the leader's own answer. At `auto`, state in one line before starting how many delegates, which model, what each does and which files each writes; at `suggest`, say the same and wait. Which model and effort a delegate runs on: `claude-agent-kit--models.md`.
- Write the shared contracts (public types, schemas, migration order, shared tests) before delegating; one writer per file (§ 14). One prompt over many inputs needs a tight template. A prompt is self-contained: files owned, expected output, what success looks like, settled decisions as constraints, what must not be done, the operating envelope (§ 9), and whether it may delegate in turn (§ 15); a nested delegate counts toward the count and files the leader stated. A delegate cannot watch a long-running process; have it write a log, a results file or an exit-code file. Implementation work never goes to a read-only delegate.

---

## Claude delegation surfaces

- `Agent` with `subagent_type` `Explore`, `Plan` or `claude-code-guide` is read-only; any other type, including `general-purpose` and `fork`, is write-capable. An `Agent` call without `model` runs on the default the prefs set (`CLAUDE_CODE_SUBAGENT_MODEL`), else on the session's model; pass `model` when the prefs default does not fit the job.
- `Workflow` orchestrates many agents. Run it only on the user's opt-in for the current turn: their own words, a skill they invoked whose instructions call it, `ultracode` confirmed by a system-reminder, or their agreement to a workflow you proposed. `ultracode` raises thoroughness; it does not remove approval, permit scope reduction, or replace your own verification of the combined result. Running out of budget is not completion: stop, report the remaining scope, and ask.
- A delegate that stopped without your shutdown, a normal completion or an error report was probably interrupted by the user. Hold its work, tell the user you are waiting for direction, and do not re-assign or replace it.
