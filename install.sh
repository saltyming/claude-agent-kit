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

RULE_FILES="
claude-agent-kit--task-execution.md
claude-agent-kit--git-workflow.md
claude-agent-kit--framework-conventions.md
claude-agent-kit--parallel-work.md
"

uninstall() {
    if [ ! -f "$MANIFEST" ]; then
        echo "No manifest found. Nothing to uninstall."
        exit 0
    fi
    while IFS= read -r f; do
        if [ -f "$f" ]; then
            case "$f" in
                *.md)
                    if head -1 "$f" | grep -q "$SIGNATURE"; then
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
    rm -f "$MANIFEST"
    if command -v claude >/dev/null 2>&1; then
        claude mcp remove workslate -s user 2>/dev/null && echo "  MCP server unregistered." || true
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

echo "Installing claude-agent-kit..."

detect_platform
mkdir -p "$RULES_DIR" "$BIN_DIR"
: > "$MANIFEST"

# Binary from latest GitHub Release
echo "Downloading workslate binary (${PLATFORM})..."
RELEASE_URL="https://github.com/${REPO}/releases/latest/download/workslate-${PLATFORM}.tar.gz"
TMP=$(mktemp -d)
download "$RELEASE_URL" "$TMP/workslate.tar.gz"
tar xzf "$TMP/workslate.tar.gz" -C "$TMP"
cp "$TMP/workslate" "$BIN_DIR/workslate"
chmod +x "$BIN_DIR/workslate"
echo "$BIN_DIR/workslate" >> "$MANIFEST"
rm -rf "$TMP"

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
echo "  Binary:  $BIN_DIR/workslate"
echo "  Config:  $CLAUDE_DIR/CLAUDE.md"
echo "  Rules:   $RULES_DIR/claude-agent-kit--*.md"
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

# Register MCP server
if command -v claude >/dev/null 2>&1; then
    echo "Registering workslate MCP server..."
    claude mcp add workslate -s user --transport stdio -- workslate 2>/dev/null && \
        echo "  MCP server registered." || \
        echo "  MCP registration failed. Add manually: claude mcp add workslate -s user --transport stdio -- workslate"
else
    echo "Claude Code CLI not found. Register MCP server manually:"
    echo "  claude mcp add workslate -s user --transport stdio -- workslate"
fi

echo ""
echo "To uninstall:"
echo "  curl -fsSL $RAW_BASE/install.sh | sh -s -- --uninstall"
