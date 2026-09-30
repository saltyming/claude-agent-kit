<!-- slate-agent-kit:common -->
# Git Workflow

Signing, model attribution, commit message format, PR body format and branch naming are the user's preferences in `$HOME/.claude/rules/claude-agent-kit--git-prefs.md` (it loads with these rules); the kit sets no default.

- A value still `unset` when needed is asked for and written into the file; only the values needed now. A missing file: ask for this action and say the file is not installed.
- When the repository's convention (recent `git log`, `CONTRIBUTING`, a PR template) differs from a prefs value, ask which to follow here and record it under "Repository overrides". The current-turn instruction outranks the file.
- Check `git branch -vv` for the base before opening a PR. Destructive git follows § 13 and never undoes session edits (§ 11).

The git prefs are the explicit user request that the system prompt's git defaults defer to. With signing set to `no-gpg-sign`, passing `--no-gpg-sign` does not violate the "never skip hooks or signing" default. With attribution set to `off`, add neither the `Co-Authored-By: Claude ...` commit trailer nor the "Generated with Claude Code" PR footer, and no other Anthropic attribution.
