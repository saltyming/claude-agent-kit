#!/bin/sh
set -e

REPO="saltyming/claude-agent-kit"
BRANCH="main"
RAW_BASE="https://raw.githubusercontent.com/${REPO}/${BRANCH}"
CLAUDE_DIR="${HOME}/.claude"
RULES_DIR="${CLAUDE_DIR}/rules"
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
"

uninstall() {
    if [ ! -f "$MANIFEST" ]; then
        echo "No manifest found. Nothing to uninstall."
        exit 0
    fi
    custom_list_file="$(mktemp)"
    while IFS= read -r f; do
        if [ -f "$f" ]; then
            case "$f" in
                *.md)
                    first="$(head -1 "$f" 2>/dev/null || true)"
                    if printf '%s' "$first" | grep -Fq "<!-- ${CUSTOM_SIGNATURE}"; then
                        printf '%s\n' "$f" >> "$custom_list_file"
                    elif printf '%s' "$first" | grep -Fq "<!-- ${SIGNATURE} -->"; then
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
    rm -f "$MANIFEST"
    if command -v claude >/dev/null 2>&1; then
        for srv in workslate aside; do
            claude mcp remove "$srv" -s user 2>/dev/null && echo "  $srv unregistered." || true
        done
    fi
    echo "Uninstalled."
    exit 0
}

for arg in "$@"; do
    case "$arg" in
        --uninstall) uninstall ;;
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
            echo "  Code signed (ad-hoc): $name." || true
    fi
    echo "$BIN_DIR/$name" >> "$MANIFEST"
    rm -rf "$tmp"
}

echo "Installing claude-agent-kit..."

detect_platform
mkdir -p "$RULES_DIR" "$BIN_DIR"
: > "$MANIFEST"

# Binaries from latest GitHub Release
install_binary workslate
install_binary aside

# CLAUDE.md
echo "Downloading CLAUDE.md..."
download "$RAW_BASE/CLAUDE.md" "$CLAUDE_DIR/CLAUDE.md"
echo "$CLAUDE_DIR/CLAUDE.md" >> "$MANIFEST"

# Rule files
echo "Downloading rules..."
for f in $RULE_FILES; do
    download "$RAW_BASE/claude-rules/$f" "$RULES_DIR/$f"
    echo "$RULES_DIR/$f" >> "$MANIFEST"
done

echo ""
echo "Installed:"
echo "  Binaries: $BIN_DIR/workslate, $BIN_DIR/aside"
echo "  Config:   $CLAUDE_DIR/CLAUDE.md"
echo "  Rules:    $RULES_DIR/claude-agent-kit--*.md"
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

# Register MCP servers
if command -v claude >/dev/null 2>&1; then
    for srv in workslate aside; do
        echo "Registering $srv MCP server..."
        claude mcp add "$srv" -s user --transport stdio -- "$srv" 2>/dev/null && \
            echo "  $srv registered." || \
            echo "  $srv registration failed. Add manually: claude mcp add $srv -s user --transport stdio -- $srv"
    done
else
    echo "Claude Code CLI not found. Register MCP servers manually:"
    echo "  claude mcp add workslate -s user --transport stdio -- workslate"
    echo "  claude mcp add aside -s user --transport stdio -- aside"
fi

# Aside preferences configuration (interactive)
echo ""
scripts_tmp=$(mktemp -d)
download "$RAW_BASE/scripts/configure-aside.sh" "$scripts_tmp/configure-aside.sh"
download "$RAW_BASE/scripts/claude-agent-kit--aside-prefs.md.tmpl" "$scripts_tmp/aside-prefs.tmpl"
CLAUDE_DIR="$CLAUDE_DIR" RULES_DIR="$RULES_DIR" MANIFEST="$MANIFEST" \
    TEMPLATE_SRC="$scripts_tmp/aside-prefs.tmpl" \
    sh "$scripts_tmp/configure-aside.sh"
rm -rf "$scripts_tmp"

echo ""
echo "To uninstall:"
echo "  curl -fsSL $RAW_BASE/install.sh | sh -s -- --uninstall"
