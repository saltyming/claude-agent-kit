#!/bin/sh
# configure-aside.sh — interactive configuration step for the `aside` MCP server.
#
# Sourced by the Makefile's `install` / `configure` targets and by install.sh
# (after the binaries and core rules are in place). Prompts the user for
# preferred backend, default models, default reasoning effort, auto-call
# policy, and an optional path to a directory of custom rule files.
#
# Reads from /dev/tty when stdin is not a terminal (so curl|sh still works).
# Every prompt is skippable via an env var; when a var is set AND non-empty,
# no prompt is shown. Unset vars in non-TTY contexts fall back to the
# documented default.
#
# Required caller env:
#   CLAUDE_DIR        e.g. $HOME/.claude
#   RULES_DIR         e.g. $HOME/.claude/rules
#   MANIFEST          path to the install manifest
#   TEMPLATE_SRC      path or URL to claude-agent-kit--aside-prefs.md.tmpl
#                     (if it looks like a path that exists, read directly;
#                      otherwise treat as URL and download)
#
# Honored env overrides (when set, suppress the corresponding prompt):
#   ASIDE_PREFERRED         none|codex|copilot
#   ASIDE_CODEX_MODEL       freeform model string or empty
#   ASIDE_COPILOT_MODEL     freeform model string or empty
#   ASIDE_CODEX_EFFORT      low|medium|high|xhigh or empty
#   ASIDE_COPILOT_EFFORT    low|medium|high|xhigh or empty
#   ASIDE_CODEX_MODEL_FALLBACK    comma-separated model list or empty
#   ASIDE_COPILOT_MODEL_FALLBACK  comma-separated model list or empty
#   ASIDE_RECONFIGURE       yes|no — override the "existing prefs found, reconfigure?"
#                           prompt. yes = overwrite; no = keep existing and exit.
#                           Unset + TTY = ask. Unset + non-TTY = keep (safe default).
#   ASIDE_POLICY            conservative|preference-only|proactive
#   ASIDE_CUSTOM_RULES_DIR  absolute path or empty
#
# Exit status: 0 on success, non-zero on unrecoverable error.

set -e

: "${CLAUDE_DIR:?configure-aside.sh: CLAUDE_DIR not set}"
: "${RULES_DIR:?configure-aside.sh: RULES_DIR not set}"
: "${MANIFEST:?configure-aside.sh: MANIFEST not set}"
: "${TEMPLATE_SRC:?configure-aside.sh: TEMPLATE_SRC not set}"

# ── shared functions ─────────────────────────────────────
# read_tty / have_tty / prompt_with_default / sed_escape / ingest_custom_rules
# live in cak-common.sh, sourced from the same directory (install.sh downloads it
# alongside this script; `make` runs it from scripts/).
. "$(dirname "$0")/cak-common.sh"

# ── existing prefs check ─────────────────────────────────

PREFS_DEST="$RULES_DIR/claude-agent-kit--aside-prefs.md"

# KEEP_PREFS=yes means the prefs file already exists and the user chose to
# preserve it — skip the prompt + sed sections below. (Custom-rules ingestion is
# a separate shared step the install flow runs once via cak-common.sh.)
KEEP_PREFS="no"

if [ -f "$PREFS_DEST" ]; then
    reconfigure=""
    if [ -n "$ASIDE_RECONFIGURE" ]; then
        case "$ASIDE_RECONFIGURE" in
            yes|YES|Yes|y|Y) reconfigure="yes" ;;
            no|NO|No|n|N)    reconfigure="no" ;;
            *)               reconfigure="" ;;
        esac
    fi
    if [ -z "$reconfigure" ]; then
        if have_tty; then
            echo "" >&2
            echo "Existing aside preferences found at:" >&2
            echo "  $PREFS_DEST" >&2
            printf "Reconfigure (overwrite)? [y/N]: " >&2
            _existing_answer=""
            if read_tty _existing_answer; then :; fi
            case "$_existing_answer" in
                y|Y|yes|YES|Yes) reconfigure="yes" ;;
                *)               reconfigure="no" ;;
            esac
        else
            reconfigure="no"
        fi
    fi
    if [ "$reconfigure" = "no" ]; then
        KEEP_PREFS="yes"
        echo "Keeping existing preferences (edit anytime at $PREFS_DEST)." >&2
        # Ensure the file is tracked in the manifest so uninstall sees it.
        if ! grep -Fxq "$PREFS_DEST" "$MANIFEST" 2>/dev/null; then
            echo "$PREFS_DEST" >> "$MANIFEST"
        fi
    fi
fi

# ── prompt (only when (re)configuring prefs) ─────────────

if [ "$KEEP_PREFS" = "no" ]; then
    echo "" >&2
    echo "Configuring claude-agent-kit aside preferences." >&2
    echo "(set ASIDE_* env vars to run fully non-interactively)" >&2
    echo "" >&2

    prompt_with_default PREFERRED_BACKEND ASIDE_PREFERRED \
        "Preferred third-party advisor [none/codex/copilot] (default: none):" \
        "none" \
        'none|codex|copilot'

    prompt_with_default CODEX_MODEL ASIDE_CODEX_MODEL \
        "Default model for codex (e.g. \"gpt-5.4\"; blank for CLI default):" \
        ""

    prompt_with_default CODEX_EFFORT ASIDE_CODEX_EFFORT \
        "Default reasoning effort for codex [low/medium/high/xhigh, blank]:" \
        "" \
        'low|medium|high|xhigh|""'

    prompt_with_default COPILOT_MODEL ASIDE_COPILOT_MODEL \
        "Default model for copilot (e.g. \"claude-sonnet-4.6\" or \"gpt-5.4\"; blank for CLI default):" \
        ""

    prompt_with_default COPILOT_EFFORT ASIDE_COPILOT_EFFORT \
        "Default reasoning effort for copilot [low/medium/high/xhigh, blank]:" \
        "" \
        'low|medium|high|xhigh|""'

    prompt_with_default CODEX_MODEL_FALLBACK ASIDE_CODEX_MODEL_FALLBACK \
        "Default model fallback chain for codex, comma-separated (blank for none):" \
        ""

    prompt_with_default COPILOT_MODEL_FALLBACK ASIDE_COPILOT_MODEL_FALLBACK \
        "Default model fallback chain for copilot, comma-separated (blank for none):" \
        ""

    prompt_with_default POLICY ASIDE_POLICY \
        "Auto-call policy [conservative/preference-only/proactive] (default: conservative):" \
        "conservative" \
        'conservative|preference-only|proactive'
fi

# ── render template (skip when keeping existing prefs) ───

if [ "$KEEP_PREFS" = "no" ]; then
    TEMPLATE_TMP=""
    cleanup_tmp() { [ -n "$TEMPLATE_TMP" ] && rm -f "$TEMPLATE_TMP" 2>/dev/null || true; }
    trap cleanup_tmp EXIT

    if [ -f "$TEMPLATE_SRC" ]; then
        TEMPLATE_CONTENT_PATH="$TEMPLATE_SRC"
    else
        TEMPLATE_TMP="$(mktemp)"
        if command -v curl >/dev/null 2>&1; then
            curl -fsSL "$TEMPLATE_SRC" -o "$TEMPLATE_TMP"
        elif command -v wget >/dev/null 2>&1; then
            wget -qO "$TEMPLATE_TMP" "$TEMPLATE_SRC"
        else
            echo "configure-aside.sh: need curl or wget to fetch $TEMPLATE_SRC" >&2
            exit 1
        fi
        TEMPLATE_CONTENT_PATH="$TEMPLATE_TMP"
    fi

    sed \
        -e "s/{{PREFERRED_BACKEND}}/$(sed_escape "$PREFERRED_BACKEND")/g" \
        -e "s/{{CODEX_MODEL}}/$(sed_escape "$CODEX_MODEL")/g" \
        -e "s/{{COPILOT_MODEL}}/$(sed_escape "$COPILOT_MODEL")/g" \
        -e "s/{{CODEX_EFFORT}}/$(sed_escape "$CODEX_EFFORT")/g" \
        -e "s/{{COPILOT_EFFORT}}/$(sed_escape "$COPILOT_EFFORT")/g" \
        -e "s/{{CODEX_MODEL_FALLBACK}}/$(sed_escape "$CODEX_MODEL_FALLBACK")/g" \
        -e "s/{{COPILOT_MODEL_FALLBACK}}/$(sed_escape "$COPILOT_MODEL_FALLBACK")/g" \
        -e "s/{{POLICY}}/$(sed_escape "$POLICY")/g" \
        "$TEMPLATE_CONTENT_PATH" > "$PREFS_DEST"

    # Record in manifest (avoid duplicate entries if re-running `make configure`).
    if ! grep -Fxq "$PREFS_DEST" "$MANIFEST" 2>/dev/null; then
        echo "$PREFS_DEST" >> "$MANIFEST"
    fi

    echo "  Wrote $PREFS_DEST" >&2
fi

# ── summary ───────────────────────────────────────────────

if [ "$KEEP_PREFS" = "yes" ]; then
    cat >&2 <<SUMMARY

Aside preferences preserved:
  preferences file:        $PREFS_DEST (unchanged)

Edit anytime:   $PREFS_DEST
Reconfigure:    make configure
SUMMARY
else
    cat >&2 <<SUMMARY

Aside preferences configured:
  preferred backend:       $PREFERRED_BACKEND
  codex model / effort:    ${CODEX_MODEL:-<CLI default>} / ${CODEX_EFFORT:-<CLI default>}
  copilot model / effort:  ${COPILOT_MODEL:-<CLI default>} / ${COPILOT_EFFORT:-<CLI default>}
  codex/copilot fallback:  ${CODEX_MODEL_FALLBACK:-<none>} / ${COPILOT_MODEL_FALLBACK:-<none>}
  auto-call policy:        $POLICY

Edit anytime:   $PREFS_DEST
Reconfigure:    make configure
SUMMARY
fi
