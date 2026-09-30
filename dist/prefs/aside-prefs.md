<!-- claude-agent-kit-custom:aside-prefs -->
# Aside Preferences

> Installed by claude-agent-kit. This file is user-owned (`claude-agent-kit-custom:aside-prefs`): uninstall and upgrade keep it. The configure step asks for each value, and you can edit any value by hand.

## Level

**suggest**

Values: `on-request` (consult only when you ask) | `suggest` (the agent proposes a consultation in one line and waits) | `auto` (the agent consults when it judges the opinion can change an open decision, and says so in one line).

## Backend

**codex**

Values: `codex` | `copilot` | `claude`. The CLI the agent consults unless you name another in the conversation.

## Codex model

****

Blank: the CLI's default model.

## Codex reasoning effort

****

Values: `low` | `medium` | `high` | `xhigh` | `max` | blank (the CLI's default).

## Codex model fallback

****

Models tried in order when the first fails transiently (rate limit, quota, model unavailable), comma-separated. Blank: none.

## Copilot model

****

Blank: the CLI's default model.

## Copilot reasoning effort

****

Values: `low` | `medium` | `high` | `xhigh` | `max` | blank.

## Copilot model fallback

****

Comma-separated. Blank: none.

## Claude model

****

Blank: the CLI's default model.

## Claude reasoning effort

****

Values: `low` | `medium` | `high` | `xhigh` | `max` | blank.

## Claude model fallback

****

Comma-separated. Blank: none.

## Notes

Free-form. Anything written here is a live rule.
