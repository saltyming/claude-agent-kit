<!-- claude-agent-kit-custom:subagent-prefs -->
# Subagent Preferences

> Installed by claude-agent-kit. This file is user-owned (`claude-agent-kit-custom:subagent-prefs`): uninstall and upgrade keep it. The configure step asks for each value and also writes the default model into the harness's own configuration; if you edit the model here by hand, run the configure step again so the harness picks it up.

## Level

**suggest**

Values: `on-request` (the agent starts subagents only when you ask) | `suggest` (the agent proposes subagents with their count, model and files in one line and waits) | `auto` (the agent starts subagents when it judges them worth their cost, and says so in one line).

## Default model

****

The model a subagent runs on unless the agent or you choose another for a call. Blank: the harness's default (usually the session's model).

## Reasoning effort

****

Claude Code has no subagent effort setting; this value is not used.

## Notes

Free-form. Anything written here is a live rule.
