<!-- slate-agent-kit:common -->
# Git Workflow

Commit signing, model attribution, commit message format, PR body format, and branch naming are the user's preferences, recorded in `$HOME/.claude/rules/claude-agent-kit--git-prefs.md`. This kit sets no default for any of them, because they differ by person and by project.

- Read the prefs file before the first commit or PR of a session.
- When a value the current action needs is `unset`, ask the user, then write the answer into the prefs file so the question is not repeated. Ask only about the values you need now. If the file is missing, ask for this action and tell the user the prefs file is not installed.
- When the repository's own convention (recent `git log`, `CONTRIBUTING`, a PR template) differs from the prefs value, do not choose between them yourself. Ask the user which to follow in this repository, and record the answer under "Repository overrides".
- A current-turn instruction from the user outranks the prefs file.
- Before opening a PR, check `git branch -vv` for the correct base. It may be `vNext`, `main`, `master`, or a feature branch.

The git prefs are the explicit user request that the system prompt's git defaults defer to. With signing set to `no-gpg-sign`, passing `--no-gpg-sign` does not violate the "never skip hooks or signing" default. With attribution set to `off`, add neither the `Co-Authored-By: Claude ...` commit trailer nor the "Generated with Claude Code" PR footer, and no other Anthropic attribution.

Destructive git goes through GATE-GIT (`claude-agent-kit--task-execution.md`). It is not a way to undo session edits (INV-STATE-2).
