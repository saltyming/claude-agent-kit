#!/bin/sh
# install.sh - installs, reconfigures or removes claude-agent-kit.
#
# This script obtains two things and runs the installer; it implements no
# installer step itself:
#   1. the kit payload: the dist/ folder beside this script when it has one,
#      otherwise the archive of the kit repository (saltyming/claude-agent-kit) at --ref;
#   2. slate-setup: built with cargo from --slate-dir when --binaries build is
#      given, otherwise the prebuilt binary for this platform from the slate
#      release v0.9.1, verified against the release's checksums.txt.
#
# Usage: sh install.sh [install|configure|uninstall] [options]
# The command is optional and defaults to install, also when the first argument
# starts with a dash. Every option is passed to slate-setup, except --ref, which
# selects the kit archive; --uninstall and --skip-mcp are kept as aliases of the
# uninstall command and of --binaries skip, and DISPATCH_ROOTS in the environment
# stands for --roots when none is given. Run `sh install.sh --help` for the list.
set -eu

KIT_NAME="claude-agent-kit"
KIT_REPO="saltyming/claude-agent-kit"
SLATE_REPO="${SLATE_RELEASE_REPO:-saltyming/slate-agent-kit}"
SLATE_VERSION="0.9.1"
RELEASE_HOST="${SLATE_RELEASE_BASE_URL:-https://github.com}"
KIT_ARCHIVE_HOST="${KIT_ARCHIVE_BASE_URL:-https://github.com}"

usage() {
  cat <<USAGE
Usage: sh install.sh [install|configure|uninstall] [options]

  install      full install or reinstall of $KIT_NAME (default)
  configure    prefs, custom rules, native configuration and server registration
  uninstall    reverse what install recorded

Options are passed to slate-setup. The common ones:
  --home DIR            harness home (default: the harness's own folder)
  --bin-dir DIR         where binaries go (default: ~/.local/bin)
  --binaries MODE       prebuilt (default), build, or skip
  --slate-dir DIR       slate checkout to build from (with --binaries build)
  --roots PATHS         workspace roots for dispatch and palette
  --set KEY=VALUE       set a prefs value, for example aside.level=auto
  --custom-rules DIR    folder of your own *.md rule files
  --payload DIR         install from this dist/ folder
  --yes                 ask nothing; take current or default values
  --dry-run             print the summary and stop
Only for this script:
  --ref REF             kit branch, tag or commit to download (default: main)
  --uninstall           same as the uninstall command
  --skip-mcp            same as --binaries skip
Environment:
  DISPATCH_ROOTS        same as --roots when --roots is not given
USAGE
}

fetch() {
  # fetch URL DEST: exit status 0 on success, 22 when the server answers 404, 1 otherwise.
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1" -o "$2" 2>/dev/null
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$2" "$1" 2>/dev/null
  else
    echo "error: curl or wget is required to download files" >&2
    exit 1
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | sed 's/^.*= *//'
  else
    return 1
  fi
}

detect_platform() {
  if [ -n "${SLATE_PLATFORM:-}" ]; then
    printf '%s' "$SLATE_PLATFORM"
    return 0
  fi
  arch=$(uname -m)
  case "$arch" in
    arm64|aarch64) arch=aarch64 ;;
    x86_64|amd64) arch=x86_64 ;;
    *) echo "error: unsupported architecture $arch; use --binaries build --slate-dir <slate checkout>" >&2; exit 1 ;;
  esac
  case "$(uname -s)" in
    Darwin) printf '%s-apple-darwin' "$arch" ;;
    Linux)
      # musl distributions cannot run the glibc binary.
      if [ -f "/lib/ld-musl-$arch.so.1" ] \
        || { command -v ldd >/dev/null 2>&1 && ldd --version 2>&1 | grep -qi musl; }; then
        printf '%s-unknown-linux-musl' "$arch"
      else
        printf '%s-unknown-linux-gnu' "$arch"
      fi
      ;;
    MINGW*|MSYS*|CYGWIN*) printf '%s-pc-windows-msvc' "$arch" ;;
    *) echo "error: unsupported system $(uname -s); use --binaries build --slate-dir <slate checkout>" >&2; exit 1 ;;
  esac
}

# --- arguments -------------------------------------------------------------
case "${1:-}" in
  install|configure|uninstall) CMD=$1; shift ;;
  *) CMD=install ;;
esac
REF=main
BINARIES=prebuilt
SLATE_DIR=
PAYLOAD_GIVEN=
ROOTS_GIVEN=
n=$#
while [ "$n" -gt 0 ]; do
  arg=$1
  shift
  n=$((n - 1))
  case "$arg" in
    -h|--help) usage; exit 0 ;;
    # Aliases for the flags earlier releases had.
    --uninstall) CMD=uninstall ;;
    --skip-mcp) BINARIES=skip; set -- "$@" --binaries skip ;;
    --ref)
      [ "$n" -gt 0 ] || { echo "error: --ref needs a value" >&2; exit 2; }
      REF=$1; shift; n=$((n - 1)) ;;
    --ref=*) REF=${arg#--ref=} ;;
    --binaries|--slate-dir|--payload)
      [ "$n" -gt 0 ] || { echo "error: $arg needs a value" >&2; exit 2; }
      val=$1; shift; n=$((n - 1))
      case "$arg" in
        --binaries) BINARIES=$val ;;
        --slate-dir) SLATE_DIR=$val ;;
        --payload) PAYLOAD_GIVEN=$val ;;
      esac
      set -- "$@" "$arg" "$val" ;;
    --roots|--roots=*) ROOTS_GIVEN=1; set -- "$@" "$arg" ;;
    --binaries=*) BINARIES=${arg#--binaries=}; set -- "$@" "$arg" ;;
    --slate-dir=*) SLATE_DIR=${arg#--slate-dir=}; set -- "$@" "$arg" ;;
    --payload=*) PAYLOAD_GIVEN=${arg#--payload=}; set -- "$@" "$arg" ;;
    *) set -- "$@" "$arg" ;;
  esac
done
# The earlier Kimi installer read DISPATCH_ROOTS; it still seeds --roots.
if [ -z "$ROOTS_GIVEN" ] && [ -n "${DISPATCH_ROOTS:-}" ]; then
  set -- "$@" --roots "$DISPATCH_ROOTS"
fi

TMP=$(mktemp -d "${TMPDIR:-/tmp}/slate-setup.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
trap 'exit 1' HUP INT TERM

# --- the kit payload -------------------------------------------------------
HERE=$(unset CDPATH; cd -- "$(dirname -- "$0")" 2>/dev/null && pwd) || HERE=.
if [ -n "$PAYLOAD_GIVEN" ]; then
  PAYLOAD=$PAYLOAD_GIVEN
elif [ -f "$HERE/dist/kit.toml" ]; then
  PAYLOAD=$HERE/dist
elif [ -f "./dist/kit.toml" ]; then
  PAYLOAD=$(pwd)/dist
else
  echo "Downloading $KIT_NAME ($REF)..."
  fetch "$KIT_ARCHIVE_HOST/$KIT_REPO/archive/$REF.tar.gz" "$TMP/kit.tar.gz" \
    || { echo "error: cannot download the $KIT_NAME archive for '$REF'" >&2; exit 1; }
  mkdir "$TMP/kit"
  tar xzf "$TMP/kit.tar.gz" -C "$TMP/kit"
  PAYLOAD=
  for d in "$TMP"/kit/*/dist; do
    [ -f "$d/kit.toml" ] && PAYLOAD=$d && break
  done
  [ -n "$PAYLOAD" ] || { echo "error: the archive for '$REF' has no dist/kit.toml" >&2; exit 1; }
fi

# --- slate-setup -----------------------------------------------------------
if [ "$BINARIES" = build ]; then
  [ -n "$SLATE_DIR" ] || { echo "error: --binaries build needs --slate-dir <slate checkout>" >&2; exit 2; }
  command -v cargo >/dev/null 2>&1 || { echo "error: cargo is required for --binaries build" >&2; exit 1; }
  echo "Building slate-setup in $SLATE_DIR..."
  cargo build --release -p slate-setup --manifest-path "$SLATE_DIR/Cargo.toml"
  SETUP=${CARGO_TARGET_DIR:-$SLATE_DIR/target}/release/slate-setup
else
  PLATFORM=$(detect_platform)
  case "$PLATFORM" in
    *windows*) EXT=zip; EXE=slate-setup.exe ;;
    *) EXT=tar.gz; EXE=slate-setup ;;
  esac
  ASSET="slate-setup-$PLATFORM.$EXT"
  BASE="$RELEASE_HOST/$SLATE_REPO/releases/download/v$SLATE_VERSION"
  LATEST="$RELEASE_HOST/$SLATE_REPO/releases/latest/download"
  echo "Downloading slate-setup ($PLATFORM)..."
  if ! fetch "$BASE/$ASSET" "$TMP/$ASSET"; then
    echo "slate release v$SLATE_VERSION has no $ASSET; using the latest release."
    BASE=$LATEST
    fetch "$BASE/$ASSET" "$TMP/$ASSET" \
      || { echo "error: cannot download $ASSET; use --binaries build --slate-dir <slate checkout>" >&2; exit 1; }
  fi
  if fetch "$BASE/checksums.txt" "$TMP/checksums.txt"; then
    expected=$(awk -v f="$ASSET" '$2 == f || $2 == "*" f { print $1; exit }' "$TMP/checksums.txt")
    if [ -z "$expected" ]; then
      echo "warning: checksums.txt has no entry for $ASSET; it is not verified." >&2
    elif actual=$(sha256_of "$TMP/$ASSET"); then
      if [ "$expected" != "$actual" ]; then
        echo "error: checksum mismatch for $ASSET" >&2
        echo "  expected: $expected" >&2
        echo "  actual:   $actual" >&2
        exit 1
      fi
    else
      echo "warning: no sha256 tool found (sha256sum, shasum or openssl); $ASSET is not verified." >&2
    fi
  else
    echo "warning: the release has no checksums.txt; $ASSET is not verified." >&2
  fi
  case "$EXT" in
    zip)
      command -v unzip >/dev/null 2>&1 || { echo "error: unzip is required" >&2; exit 1; }
      unzip -oq "$TMP/$ASSET" -d "$TMP" ;;
    *) tar xzf "$TMP/$ASSET" -C "$TMP" ;;
  esac
  SETUP=$TMP/$EXE
  chmod +x "$SETUP"
fi

# --- run -------------------------------------------------------------------
if [ -n "$PAYLOAD_GIVEN" ]; then
  set -- "$CMD" "$@"
else
  set -- "$CMD" --payload "$PAYLOAD" "$@"
fi
status=0
if [ -r /dev/tty ] && (: </dev/tty) 2>/dev/null; then
  "$SETUP" "$@" </dev/tty || status=$?
else
  "$SETUP" "$@" || status=$?
fi
exit "$status"
