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
#   ASIDE_PREFERRED         none|codex|gemini|copilot
#   ASIDE_CODEX_MODEL       freeform model string or empty
#   ASIDE_GEMINI_MODEL      freeform model string or empty
#   ASIDE_COPILOT_MODEL     freeform model string or empty
#   ASIDE_CODEX_EFFORT      low|medium|high|xhigh or empty
#   ASIDE_COPILOT_EFFORT    low|medium|high|xhigh or empty
#   ASIDE_RECONFIGURE       yes|no — override the "existing prefs found, reconfigure?"
#                           prompt. yes = overwrite; no = keep existing and exit.
#                           Unset + TTY = ask. Unset + non-TTY = keep (safe default).
#   (no ASIDE_GEMINI_EFFORT — the gemini CLI does not consume the value)
#   ASIDE_POLICY            conservative|preference-only|proactive
#   ASIDE_CUSTOM_RULES_DIR  absolute path or empty
#
# Exit status: 0 on success, non-zero on unrecoverable error.

set -e

: "${CLAUDE_DIR:?configure-aside.sh: CLAUDE_DIR not set}"
: "${RULES_DIR:?configure-aside.sh: RULES_DIR not set}"
: "${MANIFEST:?configure-aside.sh: MANIFEST not set}"
: "${TEMPLATE_SRC:?configure-aside.sh: TEMPLATE_SRC not set}"

# ── helpers ───────────────────────────────────────────────

# Read from /dev/tty when possible, fall back to stdin.
# Note: uses a `_rt_*` variable name to avoid colliding with `varname` in
# callers (POSIX sh has no local vars — any plain name would be a global).
read_tty() {
    _rt_target="$1"
    if [ -r /dev/tty ]; then
        # shellcheck disable=SC2229
        read -r "$_rt_target" < /dev/tty || return 1
    else
        # shellcheck disable=SC2229
        read -r "$_rt_target" || return 1
    fi
}

have_tty() {
    [ -r /dev/tty ] && [ -t 0 ] || [ -r /dev/tty -a ! -t 0 ] 2>/dev/null
    if [ -r /dev/tty ]; then
        return 0
    fi
    return 1
}

# Prompt with a default; respect env override if the named variable is
# already set (including empty-string via explicit assignment).
#
# Args: <var_name> <env_override_name> <prompt_text> <default_value> [<case_pattern>]
#   case_pattern: shell `case` pattern, e.g. 'none|codex|gemini|copilot'
#                 or 'low|medium|high|xhigh|""' (empty string allowed when
#                 the pattern contains ""). Empty arg = accept anything.
prompt_with_default() {
    varname="$1"
    envname="$2"
    prompt_text="$3"
    default_value="$4"
    pattern="$5"

    # If the env var is set (not just non-empty), honor it — even if empty.
    if env | grep -q "^${envname}="; then
        eval "$varname=\${$envname}"
        return 0
    fi

    if ! have_tty; then
        eval "$varname=\$default_value"
        return 0
    fi

    while :; do
        printf "%s " "$prompt_text" >&2
        if read_tty _answer; then
            :
        else
            eval "$varname=\$default_value"
            return 0
        fi
        if [ -z "$_answer" ]; then
            _answer="$default_value"
        fi
        if [ -n "$pattern" ]; then
            _match=0
            eval "case \"\$_answer\" in $pattern) _match=1 ;; esac"
            if [ "$_match" -eq 0 ]; then
                echo "  invalid value; accepted: $pattern" >&2
                continue
            fi
        fi
        eval "$varname=\$_answer"
        return 0
    done
}

# Escape a value for literal substitution via sed (handles /, &, \).
sed_escape() {
    printf '%s' "$1" | sed -e 's/[\/&]/\\&/g'
}

# ── existing prefs check ─────────────────────────────────

PREFS_DEST="$RULES_DIR/claude-agent-kit--aside-prefs.md"

# KEEP_PREFS=yes means the prefs file already exists and the user chose
# to preserve it — skip the prompt + sed sections below, but STILL run the
# custom-rules-dir prompt + ingestion at the bottom so users who want to
# "keep my prefs but add new custom rules" have a path.
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
        "Preferred third-party advisor [none/codex/gemini/copilot] (default: none):" \
        "none" \
        'none|codex|gemini|copilot'

    prompt_with_default CODEX_MODEL ASIDE_CODEX_MODEL \
        "Default model for codex (e.g. \"gpt-5.4\"; blank for CLI default):" \
        ""

    prompt_with_default CODEX_EFFORT ASIDE_CODEX_EFFORT \
        "Default reasoning effort for codex [low/medium/high/xhigh, blank]:" \
        "" \
        'low|medium|high|xhigh|""'

    prompt_with_default GEMINI_MODEL ASIDE_GEMINI_MODEL \
        "Default model for gemini (e.g. \"gemini-3.1-pro\"; blank for CLI default):" \
        ""

    prompt_with_default COPILOT_MODEL ASIDE_COPILOT_MODEL \
        "Default model for copilot (e.g. \"claude-sonnet-4.6\" or \"gpt-5.4\"; blank for CLI default):" \
        ""

    prompt_with_default COPILOT_EFFORT ASIDE_COPILOT_EFFORT \
        "Default reasoning effort for copilot [low/medium/high/xhigh, blank]:" \
        "" \
        'low|medium|high|xhigh|""'

    prompt_with_default POLICY ASIDE_POLICY \
        "Auto-call policy [conservative/preference-only/proactive] (default: conservative):" \
        "conservative" \
        'conservative|preference-only|proactive'
fi

# Custom-rules-dir prompt always runs so users who kept their prefs can
# still add custom rules in the same invocation.
prompt_with_default CUSTOM_RULES_DIR ASIDE_CUSTOM_RULES_DIR \
    "Path to a directory of your own custom rule files (blank to skip):" \
    ""

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
        -e "s/{{GEMINI_MODEL}}/$(sed_escape "$GEMINI_MODEL")/g" \
        -e "s/{{COPILOT_MODEL}}/$(sed_escape "$COPILOT_MODEL")/g" \
        -e "s/{{CODEX_EFFORT}}/$(sed_escape "$CODEX_EFFORT")/g" \
        -e "s/{{COPILOT_EFFORT}}/$(sed_escape "$COPILOT_EFFORT")/g" \
        -e "s/{{POLICY}}/$(sed_escape "$POLICY")/g" \
        "$TEMPLATE_CONTENT_PATH" > "$PREFS_DEST"

    # Record in manifest (avoid duplicate entries if re-running `make configure`).
    if ! grep -Fxq "$PREFS_DEST" "$MANIFEST" 2>/dev/null; then
        echo "$PREFS_DEST" >> "$MANIFEST"
    fi

    echo "  Wrote $PREFS_DEST" >&2
fi

# ── custom rules ingestion ────────────────────────────────

if [ -n "$CUSTOM_RULES_DIR" ]; then
    if [ ! -d "$CUSTOM_RULES_DIR" ]; then
        echo "configure-aside.sh: custom rules dir not found: $CUSTOM_RULES_DIR" >&2
        echo "  skipping custom rules ingestion" >&2
    else
        echo "Ingesting custom rules from $CUSTOM_RULES_DIR ..." >&2
        for src in "$CUSTOM_RULES_DIR"/*.md; do
            [ -f "$src" ] || continue
            base="$(basename "$src")"
            case "$base" in
                claude-agent-kit--*) dest_name="$base" ;;
                *)                   dest_name="claude-agent-kit--$base" ;;
            esac
            dest="$RULES_DIR/$dest_name"

            # Reject name collisions with core kit files (they use the
            # `claude-agent-kit` signature and would be shadowed).
            if [ -f "$dest" ] && head -1 "$dest" 2>/dev/null | grep -Fq "<!-- claude-agent-kit -->"; then
                echo "  refusing to overwrite core kit file: $dest" >&2
                continue
            fi

            first_line="$(head -1 "$src" 2>/dev/null || true)"
            if printf '%s' "$first_line" | grep -Fq "<!-- claude-agent-kit-custom"; then
                cp "$src" "$dest"
            else
                # Splice signature after the first heading (or at top if no
                # heading found) without mutating the source.
                {
                    echo "<!-- claude-agent-kit-custom:user -->"
                    cat "$src"
                } > "$dest"
            fi

            if ! grep -Fxq "$dest" "$MANIFEST" 2>/dev/null; then
                echo "$dest" >> "$MANIFEST"
            fi
            echo "  installed $dest" >&2
        done
    fi
fi

# ── summary ───────────────────────────────────────────────

if [ "$KEEP_PREFS" = "yes" ]; then
    cat >&2 <<SUMMARY

Aside preferences preserved:
  preferences file:        $PREFS_DEST (unchanged)
  custom rules dir:        ${CUSTOM_RULES_DIR:-<none>}

Edit anytime:   $PREFS_DEST
Reconfigure:    make configure
SUMMARY
else
    cat >&2 <<SUMMARY

Aside preferences configured:
  preferred backend:       $PREFERRED_BACKEND
  codex model / effort:    ${CODEX_MODEL:-<CLI default>} / ${CODEX_EFFORT:-<CLI default>}
  gemini model:            ${GEMINI_MODEL:-<CLI default>}   (effort: gemini CLI has no knob)
  copilot model / effort:  ${COPILOT_MODEL:-<CLI default>} / ${COPILOT_EFFORT:-<CLI default>}
  auto-call policy:        $POLICY
  custom rules dir:        ${CUSTOM_RULES_DIR:-<none>}

Edit anytime:   $PREFS_DEST
Reconfigure:    make configure
SUMMARY
fi
