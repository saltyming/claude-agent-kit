<!-- claude-agent-kit-custom:dispatch-prefs -->
# Dispatch Preferences

> Installed by claude-agent-kit. This file is user-owned (`claude-agent-kit-custom:dispatch-prefs`): uninstall and upgrade keep it. The configure step asks for each value, and you can edit any value by hand. The dispatch server enforces its own guards (working directory containment, sandbox ceiling, one active run per directory); they are not set here.

## Level

**suggest**

Values: `on-request` (dispatch only when you ask) | `suggest` (the agent proposes a step with its working directory and scope in one line and waits) | `auto` (the agent dispatches suitable steps and says so in one line).

## Backend

**codex**

Values: `codex` | `opencode` | `claude`.

## Model

****

Blank: the backend's default model.

## Reasoning effort

****

Values: `low` | `medium` | `high` | `xhigh` | blank (the backend's default).

## Model fallback

****

Models tried in order when the first fails transiently, comma-separated. Blank: none.

## Notes

Free-form. Anything written here is a live rule.
