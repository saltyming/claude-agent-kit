<!-- slate-agent-kit:common -->
# Model and Effort Choice

Which model and effort a subagent, a dispatch step or a consultation runs on, where the user, the prefs (§ 8) or the call has not fixed it. The table covers this harness's own models only: a dispatch step or a consultation on another vendor's backend runs on the model its prefs name, else on that backend's default. The table places the models by the vendor's own statements; a figure compares tiers on one vendor page and ranks no vendor against another.

- Choose by the shape of the work. Scoped work, whose shape is given (a named fix, a search, a mechanical edit across any number of files, a review against a checklist), runs on the cheapest tier the table lists for that kind of work, at the tier's starting effort; in the kit's own runs higher effort on such work bought time and cost, not results. Work that needs design reasoning (implementing an RFC, a change whose shape the delegate must work out) runs at high; xhigh and max only where a check shows a gain.
- A lower tier is neither a lower effort nor a smaller task: the delegate gets the same scope, the same self-contained prompt and the same verification it would get on the top model (§ 15), and never runs below its listed starting effort. The leader confirms that a check a lower tier reports was run.
- A delegate on scoped work names its model through the harness's per-call or default setting, where the harness has one, instead of inheriting the session's; an inherited top model bills every delegate's reading at the top rate.
- Parallel workers on a lower tier cut wall time when the work splits into many independent pieces; they cut cost only on routine pieces or on work too large for one context. One dependent chain stays with one model.

## Claude Code models (as of 2026-10-08)

| Model (`model`) | Tier | Use for | Start effort |
|---|---|---|---|
| Fable 5.1 (`fable`) | frontier | autonomous sessions of hours, deep multistep research | high |
| Opus 5.5 (`opus`) | default | design, architecture, review, multi-module implementation | medium; high when design-bound |
| Sonnet 5.5 (`sonnet`) | workhorse | daily coding and agentic tool use; write-capable delegates | medium; high for harder or longer work |
| Haiku 5.5 (`haiku`) | fast | read-only search and summaries, scoped coding subtasks, extraction | medium |

- Vendor figures, Terminal-Bench 4.0: Sonnet 70.6%, Haiku 39.2% (Haiku page); Opus 66.4% at xhigh, Fable 55.8% (Opus page). The vendor places Haiku as a subagent on coding work and Sonnet or Opus for long agentic coding.
- At low effort Haiku may skip a search or a check and may stop early on a long prompt.
