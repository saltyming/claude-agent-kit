#!/bin/sh
# configure-dispatch.sh — interactive configuration step for the `dispatch` MCP
# server. Mirrors configure-aside.sh; shared helpers live in cak-common.sh.
#
# Sourced-helper model: prompts for execution policy, approval mode, granularity,
# default model, and reasoning effort, then renders the dispatch-prefs template.
# Reads /dev/tty when stdin is not a terminal (so curl|sh works). Every prompt is
# skippable via a DISPATCH_* env var; when a var is set no prompt is shown.
#
# Required caller env:
#   CLAUDE_DIR     e.g. $HOME/.claude
#   RULES_DIR      e.g. $HOME/.claude/rules
#   MANIFEST       path to the install manifest
#   TEMPLATE_SRC   path or URL to claude-agent-kit--dispatch-prefs.md.tmpl
#
# Honored env overrides (when set, suppress the corresponding prompt):
#   DISPATCH_POLICY        conservative|preference-only|proactive
#   DISPATCH_APPROVAL      ask|auto
#   DISPATCH_GRANULARITY   per-step|batch|ask
#   DISPATCH_MODEL         freeform model string or empty
#   DISPATCH_EFFORT        low|medium|high|xhigh or empty
#   DISPATCH_RECONFIGURE   yes|no — override the "existing prefs found, reconfigure?"
#                          prompt. Unset + TTY = ask. Unset + non-TTY = keep.
#
# Exit status: 0 on success, non-zero on unrecoverable error.

set -e

: "${CLAUDE_DIR:?configure-dispatch.sh: CLAUDE_DIR not set}"
: "${RULES_DIR:?configure-dispatch.sh: RULES_DIR not set}"
: "${MANIFEST:?configure-dispatch.sh: MANIFEST not set}"
: "${TEMPLATE_SRC:?configure-dispatch.sh: TEMPLATE_SRC not set}"

# ── shared functions ─────────────────────────────────────
# read_tty / have_tty / prompt_with_default / sed_escape live in cak-common.sh,
# sourced from the same directory (install.sh downloads it alongside this script;
# `make` runs it from scripts/).
. "$(dirname "$0")/cak-common.sh"

# ── existing prefs check ─────────────────────────────────

PREFS_DEST="$RULES_DIR/claude-agent-kit--dispatch-prefs.md"

KEEP_PREFS="no"

if [ -f "$PREFS_DEST" ]; then
    reconfigure=""
    if [ -n "$DISPATCH_RECONFIGURE" ]; then
        case "$DISPATCH_RECONFIGURE" in
            yes|YES|Yes|y|Y) reconfigure="yes" ;;
            no|NO|No|n|N)    reconfigure="no" ;;
            *)               reconfigure="" ;;
        esac
    fi
    if [ -z "$reconfigure" ]; then
        if have_tty; then
            echo "" >&2
            echo "Existing dispatch preferences found at:" >&2
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
        echo "Keeping existing dispatch preferences (edit anytime at $PREFS_DEST)." >&2
        if ! grep -Fxq "$PREFS_DEST" "$MANIFEST" 2>/dev/null; then
            echo "$PREFS_DEST" >> "$MANIFEST"
        fi
    fi
fi

# ── prompts (only when (re)configuring) ──────────────────

if [ "$KEEP_PREFS" = "no" ]; then
    echo "" >&2
    echo "Configuring claude-agent-kit dispatch preferences." >&2
    echo "(set DISPATCH_* env vars to run fully non-interactively)" >&2
    echo "" >&2

    prompt_with_default POLICY DISPATCH_POLICY \
        "Execution policy [conservative/preference-only/proactive] (default: conservative):" \
        "conservative" \
        'conservative|preference-only|proactive'

    prompt_with_default APPROVAL DISPATCH_APPROVAL \
        "Approval mode [ask/auto] (default: ask):" \
        "ask" \
        'ask|auto'

    prompt_with_default GRANULARITY DISPATCH_GRANULARITY \
        "Default approval granularity [per-step/batch/ask] (default: ask):" \
        "ask" \
        'per-step|batch|ask'

    prompt_with_default MODEL DISPATCH_MODEL \
        "Default model for codex (e.g. \"gpt-5.5\"; blank for CLI default):" \
        ""

    prompt_with_default EFFORT DISPATCH_EFFORT \
        "Default reasoning effort [low/medium/high/xhigh, blank]:" \
        "" \
        'low|medium|high|xhigh|""'
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
            echo "configure-dispatch.sh: need curl or wget to fetch $TEMPLATE_SRC" >&2
            exit 1
        fi
        TEMPLATE_CONTENT_PATH="$TEMPLATE_TMP"
    fi

    sed \
        -e "s/{{POLICY}}/$(sed_escape "$POLICY")/g" \
        -e "s/{{APPROVAL}}/$(sed_escape "$APPROVAL")/g" \
        -e "s/{{GRANULARITY}}/$(sed_escape "$GRANULARITY")/g" \
        -e "s/{{MODEL}}/$(sed_escape "$MODEL")/g" \
        -e "s/{{EFFORT}}/$(sed_escape "$EFFORT")/g" \
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

Dispatch preferences preserved:
  preferences file:        $PREFS_DEST (unchanged)

Edit anytime:   $PREFS_DEST
Reconfigure:    make configure
SUMMARY
else
    cat >&2 <<SUMMARY

Dispatch preferences configured:
  execution policy:        $POLICY
  approval mode:           $APPROVAL
  default granularity:     $GRANULARITY
  default model / effort:  ${MODEL:-<CLI default>} / ${EFFORT:-<CLI default>}

Edit anytime:   $PREFS_DEST
Reconfigure:    make configure
SUMMARY
fi
