#!/bin/sh
# Remove what claude-agent-kit 11.x and earlier installed for workslate, the
# mid-turn messaging MCP server that 12.0.0 dropped.
#
# Owns four things and nothing else: the workslate hook entries in
# $CLAUDE_DIR/settings.json, the $BIN_DIR/workslate binary, the user-scope
# `workslate` MCP registration, and the per-project workslate.db files. Every
# other hook, server, and file is left alone. install.sh and the Makefile run it
# on both install and uninstall; it is a no-op when nothing is installed.
#
# The hook entries are the part that cannot be skipped: one of them matches every
# tool call, so with the binary gone each call reports a hook error until the
# entry is removed. The JSON edit below does that with python3 or jq. The old
# binary's own --uninstall-hooks is the last resort, and only when CLAUDE_DIR is
# the default: that binary always edits $HOME/.claude/settings.json, whatever
# CLAUDE_DIR says.
#
# Env: CLAUDE_DIR (default ~/.claude), BIN_DIR (default ~/.local/bin).
set -eu

CLAUDE_DIR="${CLAUDE_DIR:-$HOME/.claude}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
settings="$CLAUDE_DIR/settings.json"

has_workslate_hooks() {
    [ -f "$settings" ] && grep -Eq 'workslate[^"]*--hook=|\[workslate-task-verify\]' "$settings"
}

strip_with_python() {
    python3 - "$settings" <<'PY'
import json, os, shutil, sys, tempfile, time

path = sys.argv[1]
with open(path, encoding="utf-8") as fh:
    data = json.load(fh)


def ours(handler):
    if not isinstance(handler, dict):
        return False
    command = handler.get("command")
    if isinstance(command, str) and "workslate" in command and "--hook=" in command:
        return True
    prompt = handler.get("prompt")
    return (
        handler.get("type") == "agent"
        and isinstance(prompt, str)
        and prompt.startswith("[workslate-task-verify]")
    )


hooks = data.get("hooks") if isinstance(data, dict) else None
changed = False
if isinstance(hooks, dict):
    for event in list(hooks):
        groups = hooks[event]
        if not isinstance(groups, list):
            continue
        kept = []
        for group in groups:
            handlers = group.get("hooks") if isinstance(group, dict) else None
            if isinstance(handlers, list):
                remaining = [h for h in handlers if not ours(h)]
                if len(remaining) != len(handlers):
                    changed = True
                    if not remaining:
                        continue
                    group["hooks"] = remaining
            kept.append(group)
        if kept:
            hooks[event] = kept
        elif groups:
            del hooks[event]

if changed:
    shutil.copy2(path, "%s.bak-%s" % (path, time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())))
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path) or ".", prefix=".settings-")
    with os.fdopen(fd, "w", encoding="utf-8") as out:
        json.dump(data, out, indent=2, ensure_ascii=False)
        out.write("\n")
    shutil.copymode(path, tmp)
    os.replace(tmp, path)
PY
}

strip_with_jq() {
    tmp="$settings.tmp.$$"
    jq '
      def ours:
        type == "object" and (
          ((.command // "") | type == "string" and contains("workslate") and contains("--hook="))
          or (.type == "agent" and ((.prompt // "") | type == "string" and startswith("[workslate-task-verify]")))
        );
      if (.hooks | type) == "object" then
        .hooks |= with_entries(
          if (.value | type) == "array" then
            .value |= map(
              if type == "object" and ((.hooks | type) == "array") and (.hooks | any(ours)) then
                .hooks |= map(select(ours | not)) | if (.hooks | length) == 0 then empty else . end
              else . end)
          else . end)
        | .hooks |= with_entries(select((.value | type) != "array" or (.value | length) > 0))
      else . end
    ' "$settings" > "$tmp" || { rm -f "$tmp"; return 1; }
    cp -p "$settings" "$settings.bak-$(date -u +%Y%m%dT%H%M%SZ)"
    cat "$tmp" > "$settings"
    rm -f "$tmp"
}

if has_workslate_hooks; then
    if python3 -c 'import json' >/dev/null 2>&1; then
        strip_with_python || true
    elif command -v jq >/dev/null 2>&1; then
        strip_with_jq || true
    fi
    if has_workslate_hooks && [ -x "$BIN_DIR/workslate" ] && [ "$CLAUDE_DIR" = "$HOME/.claude" ]; then
        "$BIN_DIR/workslate" --uninstall-hooks >/dev/null 2>&1 || true
    fi
    if has_workslate_hooks; then
        echo "  WARNING: workslate hooks are still present in $settings and could not be removed" >&2
        echo "           automatically (needs python3 or jq, and valid JSON). Delete every hook entry" >&2
        echo "           whose command contains both 'workslate' and '--hook=', or each tool call" >&2
        echo "           will report a hook error." >&2
    else
        echo "  removed workslate hooks from $settings"
    fi
fi

if [ -e "$BIN_DIR/workslate" ]; then
    rm -f "$BIN_DIR/workslate" && echo "  removed $BIN_DIR/workslate"
fi

# `claude mcp` edits the real user config whatever CLAUDE_DIR says, so it is
# skipped for a non-default CLAUDE_DIR (a scratch or test install).
if [ "$CLAUDE_DIR" = "$HOME/.claude" ] && command -v claude >/dev/null 2>&1; then
    claude mcp remove workslate -s user >/dev/null 2>&1 && echo "  workslate MCP server unregistered." || true
fi

if [ -d "$CLAUDE_DIR/projects" ]; then
    for dir in "$CLAUDE_DIR"/projects/*/workslate; do
        [ -d "$dir" ] || continue
        rm -f "$dir"/workslate.db "$dir"/workslate.db-wal "$dir"/workslate.db-shm
        rmdir "$dir" 2>/dev/null || true
    done
fi
