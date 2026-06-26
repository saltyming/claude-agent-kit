#!/bin/sh
# cak-common.sh — shared POSIX-sh function library for the install/configure flow.
#
# SOURCED (not executed) by configure-aside.sh and configure-dispatch.sh, and by
# the install flow for the one-time custom-rules ingest. This is the shell side
# of "shared functions"; install.ps1 (Windows / PowerShell) defines its own
# equivalents — a `.sh` cannot run there.
#
# Functions read configuration from environment variables the caller sets
# (RULES_DIR, MANIFEST, etc.) and from /dev/tty so `curl | sh` stays interactive.
# No top-level side effects — sourcing only defines functions.

# Read a line into the named variable, from /dev/tty when available.
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

# True when an interactive terminal is reachable (so curl|sh can still prompt).
have_tty() {
    [ -r /dev/tty ]
}

# Prompt with a default; honor an env override if the named variable is set
# (including empty-string via explicit assignment).
#
# Args: <var_name> <env_override_name> <prompt_text> <default_value> [<case_pattern>]
#   case_pattern: shell `case` pattern, e.g. 'none|codex|copilot' or
#                 'low|medium|high|xhigh|""'. Empty arg = accept anything.
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

# Prompt once for a directory of the user's own *.md rule files and install them
# into RULES_DIR with the `claude-agent-kit--` prefix + a custom signature, so
# `make uninstall` preserves them. Shared by the aside + dispatch install flows;
# the install flow calls it ONCE. Honors CUSTOM_RULES_DIR (or, for back-compat,
# ASIDE_CUSTOM_RULES_DIR); when set even to empty, no prompt is shown.
# Requires RULES_DIR and MANIFEST in the environment.
ingest_custom_rules() {
    : "${RULES_DIR:?ingest_custom_rules: RULES_DIR not set}"
    : "${MANIFEST:?ingest_custom_rules: MANIFEST not set}"

    # Back-compat: map ASIDE_CUSTOM_RULES_DIR onto the generic var when unset, and
    # export so prompt_with_default's `env` check sees it.
    if ! env | grep -q '^CUSTOM_RULES_DIR=' && env | grep -q '^ASIDE_CUSTOM_RULES_DIR='; then
        CUSTOM_RULES_DIR="$ASIDE_CUSTOM_RULES_DIR"
        export CUSTOM_RULES_DIR
    fi

    prompt_with_default CUSTOM_RULES_DIR CUSTOM_RULES_DIR \
        "Path to a directory of your own custom rule files (blank to skip):" \
        ""

    [ -n "$CUSTOM_RULES_DIR" ] || return 0
    if [ ! -d "$CUSTOM_RULES_DIR" ]; then
        echo "ingest_custom_rules: custom rules dir not found: $CUSTOM_RULES_DIR" >&2
        echo "  skipping custom rules ingestion" >&2
        return 0
    fi

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
            # Splice the signature at the top without mutating the source.
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
}
