<!-- claude-agent-kit-custom:comment-prefs -->
# Comment Preferences

> User-owned (`claude-agent-kit-custom:comment-prefs`): uninstall and upgrade keep it. Order of precedence: the user's current-turn instruction, a "Repository overrides" line, the repository's own convention, then the value here.

## File headers

**repository**

`repository` (a header only where the repository's files carry one, in their layout) | `structured` (where the repository has no convention, every new file gets a header: one sentence of responsibility; its boundary, entry points and split with neighbors; file-wide invariants only if needed; no history, authorship or function list) | your own rule

## Comment language

**repository**

`repository` | `english` | `korean` | your own rule

## Doc comments

**repository**

`repository` | `public-api` (every public item added or changed gets a doc comment) | your own rule

## Repository overrides

One line per repository: `- <repository root or remote>: <what to follow>`.

## Notes

Free-form. Anything written here is a live rule.
