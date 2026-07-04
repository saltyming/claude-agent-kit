#!/bin/sh
set -e

REPO="saltyming/claude-agent-kit"
BRANCH="main"
RAW_BASE="https://raw.githubusercontent.com/${REPO}/${BRANCH}"
CLAUDE_DIR="${HOME}/.claude"
RULES_DIR="${CLAUDE_DIR}/rules"
SKILLS_DIR="${CLAUDE_DIR}/skills"
BIN_DIR="${HOME}/.local/bin"
MANIFEST="${CLAUDE_DIR}/.claude-agent-kit-manifest"
SIGNATURE="claude-agent-kit"
CUSTOM_SIGNATURE="claude-agent-kit-custom"

RULE_FILES="
claude-agent-kit--task-execution.md
claude-agent-kit--git-workflow.md
claude-agent-kit--framework-conventions.md
claude-agent-kit--parallel-work.md
claude-agent-kit--aside.md
claude-agent-kit--dispatch.md
claude-agent-kit--palette.md
"

SKILL_FILES="
palette-init
palette-spec
palette-ux
palette-ui
palette-rules
"

uninstall() {
    if [ ! -f "$MANIFEST" ]; then
        echo "No manifest found. Nothing to uninstall."
        exit 0
    fi
    if [ -x "$BIN_DIR/workslate" ]; then
        "$BIN_DIR/workslate" --uninstall-hooks 2>/dev/null || true
    fi
    custom_list_file="$(mktemp)"
    while IFS= read -r f; do
        if [ -f "$f" ]; then
            case "$f" in
                *.md)
                    first="$(head -1 "$f" 2>/dev/null || true)"
                    if printf '%s' "$first" | grep -Fq "<!-- ${CUSTOM_SIGNATURE}"; then
                        printf '%s\n' "$f" >> "$custom_list_file"
                    elif printf '%s' "$first" | grep -Eq "<!-- (slate-agent-kit:common|${SIGNATURE}) -->"; then
                        rm -f "$f"
                        echo "  removed $f"
                    else
                        echo "  skipped $f (signature mismatch)"
                    fi ;;
                *)
                    rm -f "$f"
                    echo "  removed $f" ;;
            esac
        fi
    done < "$MANIFEST"

    if [ -s "$custom_list_file" ]; then
        echo ""
        echo "The following user-owned files were installed alongside the kit:"
        sed 's/^/  /' "$custom_list_file"
        keep="yes"
        if [ -n "$ASIDE_UNINSTALL_KEEP_PREFS" ]; then
            case "$ASIDE_UNINSTALL_KEEP_PREFS" in
                no|NO|No|n|N) keep="no" ;;
                *)            keep="yes" ;;
            esac
        elif [ -r /dev/tty ]; then
            printf "Remove these too? [y/N]: " > /dev/tty
            read answer < /dev/tty || answer=""
            case "$answer" in
                y|Y|yes|YES|Yes) keep="no" ;;
                *)               keep="yes" ;;
            esac
        fi
        if [ "$keep" = "no" ]; then
            while IFS= read -r f; do
                [ -z "$f" ] && continue
                rm -f "$f" && echo "  removed $f"
            done < "$custom_list_file"
        else
            echo ""
            echo "Preserved (not managed by claude-agent-kit from this point on):"
            sed 's/^/  /' "$custom_list_file"
        fi
    fi
    rm -f "$custom_list_file"
    # Remove palette skill directories recorded in the manifest (core-signed only)
    grep -E '/skills/palette-' "$MANIFEST" 2>/dev/null | while IFS= read -r d; do
        if [ -d "$d" ] && [ -f "$d/SKILL.md" ] && head -8 "$d/SKILL.md" 2>/dev/null | grep -Eq "<!-- (slate-agent-kit:common|${SIGNATURE}) -->"; then
            rm -rf "$d" && echo "  removed $d"
        elif [ -e "$d" ]; then
            echo "  skipped $d (signature mismatch)"
        fi
    done
    rm -f "$MANIFEST"
    if command -v claude >/dev/null 2>&1; then
        for srv in workslate aside dispatch; do
            claude mcp remove "$srv" -s user 2>/dev/null && echo "  $srv unregistered." || true
        done
    fi
    echo "Uninstalled."
    exit 0
}

for arg in "$@"; do
    case "$arg" in
        --uninstall) uninstall ;;
        --skip-mcp) SKIP_MCP=1 ;;
        -h|--help)
            echo "Usage: $0 [--uninstall] [--skip-mcp]"
            echo "  --uninstall   remove kit-signed files (user-owned '-custom:' prefs are kept)"
            echo "  --skip-mcp    install rules/skills/workslate only; skip shared aside/dispatch"
            echo "Env: SKIP_MCP=1, SLATE_AGENT_KIT_DIR=<path>, CLAUDE_DIR=<path>, BIN_DIR=<path>,"
            echo "     SLATE_REPO / SLATE_BRANCH, ASIDE_* / DISPATCH_* prefs, CUSTOM_RULES_DIR"
            exit 0 ;;
    esac
done

detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        darwin) OS="apple-darwin" ;;
        linux)  OS="unknown-linux-gnu" ;;
        *)      echo "Unsupported OS: $OS"; exit 1 ;;
    esac

    case "$ARCH" in
        x86_64)  ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac

    PLATFORM="${ARCH}-${OS}"
}

download() {
    url="$1"
    dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$dest"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
    else
        echo "Error: curl or wget required"
        exit 1
    fi
}

install_binary() {
    name="$1"
    echo "Downloading $name binary (${PLATFORM})..."
    url="https://github.com/${REPO}/releases/latest/download/${name}-${PLATFORM}.tar.gz"
    tmp=$(mktemp -d)
    download "$url" "$tmp/${name}.tar.gz"
    tar xzf "$tmp/${name}.tar.gz" -C "$tmp"
    cp "$tmp/$name" "$BIN_DIR/$name"
    chmod +x "$BIN_DIR/$name"
    if [ "$(uname -s)" = "Darwin" ] && command -v codesign >/dev/null 2>&1; then
        codesign --force --sign - "$BIN_DIR/$name" 2>/dev/null && \
            echo "  Code signed (ad-hoc): $name." || \
            echo "  WARNING: codesign failed for $name; macOS may SIGKILL the unsigned binary on launch." >&2
    fi
    echo "$BIN_DIR/$name" >> "$MANIFEST"
    rm -rf "$tmp"
}

echo "Installing claude-agent-kit..."

detect_platform
mkdir -p "$RULES_DIR" "$BIN_DIR"
: > "$MANIFEST"

# workslate binary from the latest GitHub Release (aside/dispatch are shared
# slate-agent-kit crates — built and registered by install_shared_mcp below)
install_binary workslate

# Register PreToolUse doorbell hooks in settings.json
"$BIN_DIR/workslate" --install-hooks || echo "  Hook registration failed. Run manually: $BIN_DIR/workslate --install-hooks"

# CLAUDE.md — back up an existing unmanaged file before overwriting it
if [ -f "$CLAUDE_DIR/CLAUDE.md" ] && ! head -1 "$CLAUDE_DIR/CLAUDE.md" | grep -Eq "<!-- (slate-agent-kit:common|${SIGNATURE}) -->"; then
    bak="$CLAUDE_DIR/CLAUDE.md.bak-$(date -u +%Y%m%dT%H%M%SZ)"
    cp -p "$CLAUDE_DIR/CLAUDE.md" "$bak"
    echo "  WARNING: existing $CLAUDE_DIR/CLAUDE.md is not managed by this kit; backed up to $bak"
    echo "## backup: $bak" >> "$MANIFEST"
fi
echo "Downloading CLAUDE.md..."
download "$RAW_BASE/CLAUDE.md" "$CLAUDE_DIR/CLAUDE.md"
echo "$CLAUDE_DIR/CLAUDE.md" >> "$MANIFEST"

# Rule files
echo "Downloading rules..."
for f in $RULE_FILES; do
    download "$RAW_BASE/claude-rules/$f" "$RULES_DIR/$f"
    echo "$RULES_DIR/$f" >> "$MANIFEST"
done

# Palette skills (each is a directory holding one SKILL.md)
echo "Downloading palette skills..."
for s in $SKILL_FILES; do
    mkdir -p "$SKILLS_DIR/$s"
    download "$RAW_BASE/claude-skills/$s/SKILL.md" "$SKILLS_DIR/$s/SKILL.md"
    echo "$SKILLS_DIR/$s" >> "$MANIFEST"
done

echo ""
echo "Installed:"
echo "  Binary:   $BIN_DIR/workslate (aside/dispatch installed via slate-agent-kit below)"
echo "  Config:   $CLAUDE_DIR/CLAUDE.md"
echo "  Rules:    $RULES_DIR/claude-agent-kit--*.md"
echo "  Skills:   $SKILLS_DIR/palette-*"
echo ""

# PATH check
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        echo "WARNING: $BIN_DIR is not in your PATH."
        echo ""
        SHELL_NAME=$(basename "${SHELL:-/bin/sh}")
        case "$SHELL_NAME" in
            zsh)  RC="~/.zshrc" ;;
            bash) RC="~/.bashrc" ;;
            fish) RC="~/.config/fish/config.fish" ;;
            *)    RC="your shell config" ;;
        esac
        echo "  Add it by running:"
        if [ "$SHELL_NAME" = "fish" ]; then
            echo "    fish_add_path $BIN_DIR"
        else
            echo "    echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> $RC"
            echo "    source $RC"
        fi
        echo "" ;;
esac

# Register the workslate MCP server (Claude-only)
if command -v claude >/dev/null 2>&1; then
    echo "Registering workslate MCP server..."
    claude mcp add workslate -s user --transport stdio -- "$BIN_DIR/workslate" 2>/dev/null && \
        echo "  workslate registered." || \
        echo "  workslate registration failed. Add manually: claude mcp add workslate -s user --transport stdio -- $BIN_DIR/workslate"
else
    echo "Claude Code CLI not found. Register manually:"
    echo "  claude mcp add workslate -s user --transport stdio -- $BIN_DIR/workslate"
fi

# Build + register the SHARED aside/dispatch servers from slate-agent-kit
find_slate_dir() {
    if [ -n "${SLATE_AGENT_KIT_DIR:-}" ] && [ -x "$SLATE_AGENT_KIT_DIR/tooling/install-mcp.sh" ]; then
        printf '%s' "$SLATE_AGENT_KIT_DIR"
        return 0
    fi
    for candidate in "../slate-agent-kit" "../.."; do
        if [ -x "$candidate/tooling/install-mcp.sh" ]; then
            (CDPATH= cd -- "$candidate" && pwd)
            return 0
        fi
    done
    return 1
}

install_shared_mcp() {
    if [ "${SKIP_MCP:-0}" = "1" ]; then
        echo "Skipping shared aside/dispatch installation because SKIP_MCP=1."
        return 0
    fi
    if slate_dir="$(find_slate_dir 2>/dev/null)"; then
        BIN_DIR="$BIN_DIR" CLAUDE_DIR="$CLAUDE_DIR" "$slate_dir/tooling/install-mcp.sh" --configure-claude
        return 0
    fi
    command -v git >/dev/null 2>&1 || {
        echo "Error: git is required to fetch slate-agent-kit for aside/dispatch. Re-run with SKIP_MCP=1 to install workslate + rules only." >&2
        exit 1
    }
    slate_tmp=$(mktemp -d)
    git clone --depth=1 --branch "${SLATE_BRANCH:-main}" "https://github.com/${SLATE_REPO:-saltyming/slate-agent-kit}.git" "$slate_tmp/slate-agent-kit"
    BIN_DIR="$BIN_DIR" CLAUDE_DIR="$CLAUDE_DIR" "$slate_tmp/slate-agent-kit/tooling/install-mcp.sh" --configure-claude
    rm -rf "$slate_tmp"
}

install_shared_mcp

# Interactive aside + dispatch preference configuration — the SAME
# configure-prefs.sh codex/kimi use (single source). Templates must sit next to
# it (it resolves "$HERE/<PREFIX>--{aside,dispatch}-prefs.md.tmpl").
echo ""
scripts_tmp=$(mktemp -d)
download "$RAW_BASE/scripts/configure-prefs.sh" "$scripts_tmp/configure-prefs.sh"
download "$RAW_BASE/scripts/cak-common.sh" "$scripts_tmp/cak-common.sh"
download "$RAW_BASE/scripts/claude-agent-kit--aside-prefs.md.tmpl" "$scripts_tmp/claude-agent-kit--aside-prefs.md.tmpl"
download "$RAW_BASE/scripts/claude-agent-kit--dispatch-prefs.md.tmpl" "$scripts_tmp/claude-agent-kit--dispatch-prefs.md.tmpl"
RULES_DIR="$RULES_DIR" PREFIX=claude-agent-kit MANIFEST="$MANIFEST" \
    sh "$scripts_tmp/configure-prefs.sh"
# Custom-rules ingestion (claude-specific; separate concern from prefs)
RULES_DIR="$RULES_DIR" MANIFEST="$MANIFEST" \
    sh -c ". \"$scripts_tmp/cak-common.sh\"; ingest_custom_rules"
rm -rf "$scripts_tmp"

echo ""
echo "To uninstall:"
echo "  curl -fsSL $RAW_BASE/install.sh | sh -s -- --uninstall"
