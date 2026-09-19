#!/bin/sh
# Generate or update the aside, dispatch, git, and comment preference files
# for a rendered kit.
#
# Harness-neutral: the kit's Makefile/install.sh invokes this with
#   RULES_DIR=<harness rules dir> PREFIX=<kit prefix> [MANIFEST=<file>] \
#     sh scripts/configure-prefs.sh
# Templates are looked up next to this script as
#   <PREFIX>--{aside,dispatch,git,comment}-prefs.md.tmpl
# (rendered from shared/prefs/ by slate's render-kit.sh).
#
# INTERACTIVE-FIRST. On a terminal (read from /dev/tty, so `curl … | sh` still
# works) this ASKS before changing existing prefs ("Reconfigure? [y/N]") and
# then prompts each knob. Environment variables are an OPTIONAL non-interactive
# escape hatch only: they seed a prompt's default, and PREFS_RECONFIGURE /
# PREFS_PROMPT let CI run without a TTY. Nothing here *requires* an env var.
# Existing prefs carry a `-custom:` signature and are user-owned; the default is
# always to keep them unless the user says otherwise.
#
# aside/dispatch are regenerated from their template. git/comment are edited in
# place: the agent records answers and the user writes repository overrides and
# notes into those files, so a reconfigure rewrites only the value lines and
# offers each current value as the prompt default.
set -eu

HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
: "${RULES_DIR:?RULES_DIR is required}"
: "${PREFIX:?PREFIX is required}"
MANIFEST="${MANIFEST:-}"

# Optional env seeds for the prompts (blank = "no default"). Back-compat: older
# kits used ASIDE_*_MODEL_FALLBACK; accept it as an alias for ASIDE_*_FALLBACK.
: "${ASIDE_CODEX_FALLBACK:=${ASIDE_CODEX_MODEL_FALLBACK:-}}"
: "${ASIDE_COPILOT_FALLBACK:=${ASIDE_COPILOT_MODEL_FALLBACK:-}}"
: "${ASIDE_CLAUDE_FALLBACK:=${ASIDE_CLAUDE_MODEL_FALLBACK:-}}"

ASIDE_PREFERRED="${ASIDE_PREFERRED:-none}"
ASIDE_CODEX_MODEL="${ASIDE_CODEX_MODEL:-}"
ASIDE_COPILOT_MODEL="${ASIDE_COPILOT_MODEL:-}"
ASIDE_CLAUDE_MODEL="${ASIDE_CLAUDE_MODEL:-}"
ASIDE_CODEX_EFFORT="${ASIDE_CODEX_EFFORT:-}"
ASIDE_COPILOT_EFFORT="${ASIDE_COPILOT_EFFORT:-}"
ASIDE_CLAUDE_EFFORT="${ASIDE_CLAUDE_EFFORT:-}"
ASIDE_CODEX_FALLBACK="${ASIDE_CODEX_FALLBACK:-}"
ASIDE_COPILOT_FALLBACK="${ASIDE_COPILOT_FALLBACK:-}"
ASIDE_CLAUDE_FALLBACK="${ASIDE_CLAUDE_FALLBACK:-}"
ASIDE_POLICY="${ASIDE_POLICY:-conservative}"

DISPATCH_POLICY="${DISPATCH_POLICY:-conservative}"
DISPATCH_APPROVAL="${DISPATCH_APPROVAL:-ask}"
DISPATCH_GRANULARITY="${DISPATCH_GRANULARITY:-ask}"
DISPATCH_BACKEND="${DISPATCH_BACKEND:-}"
DISPATCH_MODEL="${DISPATCH_MODEL:-}"
DISPATCH_EFFORT="${DISPATCH_EFFORT:-}"
DISPATCH_FALLBACK="${DISPATCH_FALLBACK:-}"

# git/comment seeds are blank by default: a blank seed means "use the value
# already in the file (or the template)", not "set it blank".
GIT_SIGNING="${GIT_SIGNING:-}"
GIT_ATTRIBUTION="${GIT_ATTRIBUTION:-}"
GIT_COMMIT_FORMAT="${GIT_COMMIT_FORMAT:-}"
GIT_PR_BODY="${GIT_PR_BODY:-}"
GIT_BRANCH_NAMING="${GIT_BRANCH_NAMING:-}"
COMMENT_HEADERS="${COMMENT_HEADERS:-}"
COMMENT_LANGUAGE="${COMMENT_LANGUAGE:-}"
COMMENT_DOC="${COMMENT_DOC:-}"

# Prompt from the controlling terminal so `curl | sh` (where stdin is the
# script) can still interact.
have_tty() { [ -r /dev/tty ] && [ -w /dev/tty ]; }
interactive() { [ "${PREFS_PROMPT:-yes}" != "no" ] && have_tty; }

prompt_var() {
  # prompt_var VARNAME "label" — show the current value as the default; a blank
  # answer keeps it. Only called when interactive.
  _pv_name="$1"
  _pv_label="$2"
  eval "_pv_cur=\${$_pv_name}"
  printf '%s [%s]: ' "$_pv_label" "$_pv_cur" > /dev/tty
  IFS= read -r _pv_ans < /dev/tty || _pv_ans=""
  if [ -n "$_pv_ans" ]; then
    eval "$_pv_name=\$_pv_ans"
  fi
}

# want_generate <dest> [question] — returns 0 to (re)generate, 1 to keep. New
# files are always generated; for an existing file the user is ASKED (default:
# keep). PREFS_RECONFIGURE overrides the prompt for non-interactive runs.
want_generate() {
  _wg_dest="$1"
  _wg_question="${2:-Reconfigure (overwrite)? [y/N]: }"
  [ -f "$_wg_dest" ] || return 0
  case "${PREFS_RECONFIGURE:-}" in
    yes|YES|Yes|y|Y) return 0 ;;
    no|NO|No|n|N) return 1 ;;
  esac
  interactive || return 1
  echo "" > /dev/tty
  echo "Existing preferences found at:" > /dev/tty
  echo "  $_wg_dest" > /dev/tty
  printf '%s' "$_wg_question" > /dev/tty
  IFS= read -r _wg_ans < /dev/tty || _wg_ans=""
  case "$_wg_ans" in
    y|Y|yes|YES|Yes) return 0 ;;
    *) return 1 ;;
  esac
}

record_manifest() {
  [ -n "$MANIFEST" ] || return 0
  grep -Fxq "$1" "$MANIFEST" 2>/dev/null || echo "$1" >> "$MANIFEST"
}

# Escape a value for the RHS of `s|@@X@@|VALUE|g`: backslash first (so the ones
# we add are not re-escaped), then the '&' back-reference and the '|' delimiter.
# Without this a model id / fallback chain containing | & or \ corrupts the
# substitution (or aborts under set -e).
sed_escape() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/&/\\&/g' -e 's/|/\\|/g'
}

render_aside() {
  mkdir -p "$(dirname -- "$1")"
  sed \
    -e "s|@@ASIDE_PREFERRED@@|$(sed_escape "$ASIDE_PREFERRED")|g" \
    -e "s|@@ASIDE_CODEX_MODEL@@|$(sed_escape "$ASIDE_CODEX_MODEL")|g" \
    -e "s|@@ASIDE_COPILOT_MODEL@@|$(sed_escape "$ASIDE_COPILOT_MODEL")|g" \
    -e "s|@@ASIDE_CLAUDE_MODEL@@|$(sed_escape "$ASIDE_CLAUDE_MODEL")|g" \
    -e "s|@@ASIDE_CODEX_EFFORT@@|$(sed_escape "$ASIDE_CODEX_EFFORT")|g" \
    -e "s|@@ASIDE_COPILOT_EFFORT@@|$(sed_escape "$ASIDE_COPILOT_EFFORT")|g" \
    -e "s|@@ASIDE_CLAUDE_EFFORT@@|$(sed_escape "$ASIDE_CLAUDE_EFFORT")|g" \
    -e "s|@@ASIDE_CODEX_FALLBACK@@|$(sed_escape "$ASIDE_CODEX_FALLBACK")|g" \
    -e "s|@@ASIDE_COPILOT_FALLBACK@@|$(sed_escape "$ASIDE_COPILOT_FALLBACK")|g" \
    -e "s|@@ASIDE_CLAUDE_FALLBACK@@|$(sed_escape "$ASIDE_CLAUDE_FALLBACK")|g" \
    -e "s|@@ASIDE_POLICY@@|$(sed_escape "$ASIDE_POLICY")|g" \
    "$2" > "$1"
  echo "  prefs: $1"
  record_manifest "$1"
}

render_dispatch() {
  mkdir -p "$(dirname -- "$1")"
  sed \
    -e "s|@@DISPATCH_POLICY@@|$(sed_escape "$DISPATCH_POLICY")|g" \
    -e "s|@@DISPATCH_APPROVAL@@|$(sed_escape "$DISPATCH_APPROVAL")|g" \
    -e "s|@@DISPATCH_GRANULARITY@@|$(sed_escape "$DISPATCH_GRANULARITY")|g" \
    -e "s|@@DISPATCH_BACKEND@@|$(sed_escape "$DISPATCH_BACKEND")|g" \
    -e "s|@@DISPATCH_MODEL@@|$(sed_escape "$DISPATCH_MODEL")|g" \
    -e "s|@@DISPATCH_EFFORT@@|$(sed_escape "$DISPATCH_EFFORT")|g" \
    -e "s|@@DISPATCH_FALLBACK@@|$(sed_escape "$DISPATCH_FALLBACK")|g" \
    "$2" > "$1"
  echo "  prefs: $1"
  record_manifest "$1"
}

aside_tmpl="$HERE/${PREFIX}--aside-prefs.md.tmpl"
dispatch_tmpl="$HERE/${PREFIX}--dispatch-prefs.md.tmpl"
aside_dest="$RULES_DIR/${PREFIX}--aside-prefs.md"
dispatch_dest="$RULES_DIR/${PREFIX}--dispatch-prefs.md"
[ -f "$aside_tmpl" ] || { echo "Error: template not found: $aside_tmpl" >&2; exit 1; }
[ -f "$dispatch_tmpl" ] || { echo "Error: template not found: $dispatch_tmpl" >&2; exit 1; }

# ── aside ─────────────────────────────────────────────────
if want_generate "$aside_dest"; then
  if interactive; then
    echo "Configuring aside preferences (Enter keeps the shown default)." > /dev/tty
    prompt_var ASIDE_PREFERRED      "aside preferred backend (none/codex/copilot/claude)"
    prompt_var ASIDE_CODEX_MODEL    "aside codex default model (blank = CLI default)"
    prompt_var ASIDE_CODEX_EFFORT   "aside codex reasoning effort (low/medium/high/xhigh, blank)"
    prompt_var ASIDE_COPILOT_MODEL  "aside copilot default model (blank = CLI default)"
    prompt_var ASIDE_COPILOT_EFFORT "aside copilot reasoning effort (low/medium/high/xhigh, blank)"
    prompt_var ASIDE_CLAUDE_MODEL   "aside claude default model (blank = CLI default)"
    prompt_var ASIDE_CLAUDE_EFFORT  "aside claude reasoning effort (low/medium/high/xhigh/max, blank)"
    prompt_var ASIDE_CODEX_FALLBACK   "aside codex fallback chain, comma-separated (blank = none)"
    prompt_var ASIDE_COPILOT_FALLBACK "aside copilot fallback chain, comma-separated (blank = none)"
    prompt_var ASIDE_CLAUDE_FALLBACK  "aside claude fallback chain, comma-separated (blank = none)"
    prompt_var ASIDE_POLICY         "aside auto-call policy (conservative/preference-only/proactive)"
  fi
  render_aside "$aside_dest" "$aside_tmpl"
else
  echo "  prefs exist, keeping: $aside_dest"
  record_manifest "$aside_dest"
fi

# ── dispatch ──────────────────────────────────────────────
if want_generate "$dispatch_dest"; then
  if interactive; then
    echo "Configuring dispatch preferences (Enter keeps the shown default)." > /dev/tty
    prompt_var DISPATCH_POLICY      "dispatch execution policy (conservative/preference-only/proactive)"
    prompt_var DISPATCH_APPROVAL    "dispatch approval mode (ask/auto)"
    prompt_var DISPATCH_GRANULARITY "dispatch default granularity (per-step/batch/ask)"
    prompt_var DISPATCH_BACKEND     "dispatch default backend (codex/opencode/claude, blank)"
    prompt_var DISPATCH_MODEL       "dispatch default model (blank = backend default)"
    prompt_var DISPATCH_EFFORT      "dispatch default reasoning effort (low/medium/high/xhigh, blank)"
    prompt_var DISPATCH_FALLBACK    "dispatch fallback chain, comma-separated (blank = none)"
  fi
  render_dispatch "$dispatch_dest" "$dispatch_tmpl"
else
  echo "  prefs exist, keeping: $dispatch_dest"
  record_manifest "$dispatch_dest"
fi

# ── git / comment (edited in place) ───────────────────────
# A value line is the first `**...**` line under its `## <heading>`.

# A file edited on Windows has CRLF endings; matching ignores the trailing \r
# and a rewritten line keeps it.

# read_value <file> <heading> — print the current value under <heading>.
read_value() {
  PV_HEADING="$2" awk '
    { l = $0; sub(/\r$/, "", l) }
    l == "## " ENVIRON["PV_HEADING"] { on = 1; next }
    on && l ~ /^## / { exit }
    on && l ~ /^\*\*.*\*\*$/ { sub(/^\*\*/, "", l); sub(/\*\*$/, "", l); print l; exit }
  ' "$1"
}

# write_value <file> <heading> <value> — replace the value line under
# <heading>, leaving every other line as it is. Values travel through ENVIRON,
# not `awk -v`, so backslashes and other metacharacters are written literally.
write_value() {
  _wv_tmp="$1.tmp.$$"
  PV_HEADING="$2" PV_VALUE="$3" awk '
    { l = $0; cr = sub(/\r$/, "", l) ? "\r" : "" }
    l == "## " ENVIRON["PV_HEADING"] { on = 1; print; next }
    on && l ~ /^## / { on = 0 }
    on && l ~ /^\*\*.*\*\*$/ { print "**" ENVIRON["PV_VALUE"] "**" cr; on = 0; next }
    { print }
  ' "$1" > "$_wv_tmp"
  mv "$_wv_tmp" "$1"
}

# configure_in_place <template> <dest> <title> <VAR|heading|prompt label>...
# A new file starts as a copy of the template; an existing file is changed only
# if the user agrees to reconfigure it. Each knob's prompt default is its env
# seed if set, otherwise the value already in the file.
configure_in_place() {
  _ci_tmpl="$1"; _ci_dest="$2"; _ci_title="$3"; shift 3
  [ -f "$_ci_tmpl" ] || { echo "Error: template not found: $_ci_tmpl" >&2; exit 1; }
  if ! want_generate "$_ci_dest" 'Reconfigure the values (other lines are kept)? [y/N]: '; then
    echo "  prefs exist, keeping: $_ci_dest"
    record_manifest "$_ci_dest"
    return 0
  fi
  _ci_work="$_ci_dest.work.$$"
  mkdir -p "$(dirname -- "$_ci_dest")"
  if [ -f "$_ci_dest" ]; then cp "$_ci_dest" "$_ci_work"; else cp "$_ci_tmpl" "$_ci_work"; fi
  if interactive; then
    echo "Configuring $_ci_title preferences (Enter keeps the shown value)." > /dev/tty
  fi
  _ci_val=""
  for _ci_spec in "$@"; do
    _ci_var=${_ci_spec%%|*}
    _ci_rest=${_ci_spec#*|}
    _ci_heading=${_ci_rest%%|*}
    _ci_label=${_ci_rest#*|}
    eval "_ci_seed=\${$_ci_var}"
    if [ -z "$_ci_seed" ]; then
      _ci_seed=$(read_value "$_ci_work" "$_ci_heading")
      eval "$_ci_var=\$_ci_seed"
    fi
    if interactive; then
      prompt_var "$_ci_var" "$_ci_label"
    fi
    eval "_ci_val=\${$_ci_var}"
    write_value "$_ci_work" "$_ci_heading" "$_ci_val"
  done
  mv "$_ci_work" "$_ci_dest"
  echo "  prefs: $_ci_dest"
  record_manifest "$_ci_dest"
}

configure_in_place "$HERE/${PREFIX}--git-prefs.md.tmpl" "$RULES_DIR/${PREFIX}--git-prefs.md" git \
  "GIT_SIGNING|Commit signing|git commit signing (default/no-gpg-sign, unset = ask later)" \
  "GIT_ATTRIBUTION|Model attribution|git model attribution (on/off, unset = ask later)" \
  "GIT_COMMIT_FORMAT|Commit message format|git commit message format (conventional/repository/your own, unset = ask later)" \
  "GIT_PR_BODY|PR body format|git PR body format (summary-test-plan/repository/your own, unset = ask later)" \
  "GIT_BRANCH_NAMING|Branch naming|git branch naming (descriptive/repository/your own, unset = ask later)"

configure_in_place "$HERE/${PREFIX}--comment-prefs.md.tmpl" "$RULES_DIR/${PREFIX}--comment-prefs.md" comment \
  "COMMENT_HEADERS|File headers|file headers (repository/structured/your own)" \
  "COMMENT_LANGUAGE|Comment language|comment language (repository/english/korean/your own)" \
  "COMMENT_DOC|Doc comments|doc comments (repository/public-api/your own)"
