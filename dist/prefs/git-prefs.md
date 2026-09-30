<!-- claude-agent-kit-custom:git-prefs -->
# Git Preferences

> User-owned (`claude-agent-kit-custom:git-prefs`): uninstall and upgrade keep it. `unset` is asked for before the first commit or PR that needs it, then written here.

## Commit signing

**unset**

`default` | `no-gpg-sign` (pass `--no-gpg-sign` on `commit`, `commit --amend`, `revert`, `cherry-pick`)

## Model attribution

**unset**

`on` | `off` (no `Co-Authored-By:` naming a model, no "Generated with ..." footer, no line identifying the agent)

## Commit message format

**unset**

`conventional` (`<type>(<area>): <subject>`, blank line, body; types `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `perf`) | `repository` | your own format

## PR body format

**unset**

`summary-test-plan` (`## Summary` bullets, then a `## Test plan` checklist) | `repository` | your own format

## Branch naming

**unset**

`descriptive` (`feat/...`, `fix/...`; never push a worktree branch name as is) | `repository` | your own rule

## Repository overrides

One line per repository where its convention differs from a value above: `- <repository root or remote>: <what to follow>`.

## Notes

Free-form. Anything written here is a live rule.
