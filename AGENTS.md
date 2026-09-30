# claude-agent-kit: maintaining this repository

This repository is rendered from slate-agent-kit (`https://github.com/saltyming/slate-agent-kit`). These instructions are for working on the repository itself; the manual the kit installs is `dist/CLAUDE.md`, and it does not govern work here.

- `dist/`, `install.sh`, `install.ps1` and `Makefile` are rendered by slate's `tooling/render-kit.sh claude`. Do not edit them by hand: change the sources in slate (`shared/`, `adapters/claude/`, `tooling/kit-scripts/`), re-render, and run slate's `tooling/validate.sh`.
- `README.md`, `CHANGELOG.md` and `LICENSE.md` are maintained here.
- Releases follow slate's release train (slate `CLAUDE.md`, *Release train*): this repository is tagged after slate's CI is green and before slate is tagged.
