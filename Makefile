CLAUDE_DIR := $(HOME)/.claude
RULES_DIR  := $(CLAUDE_DIR)/rules
BIN_DIR    := $(HOME)/.local/bin
MANIFEST   := $(CLAUDE_DIR)/.claude-agent-kit-manifest
SIGNATURE  := claude-agent-kit

RULE_FILES := $(wildcard claude-rules/*.md)

.PHONY: install uninstall build

build:
	cargo build --release -p workslate

install: build
	@mkdir -p $(RULES_DIR) $(BIN_DIR)
	@: > $(MANIFEST)
	cp CLAUDE.md $(CLAUDE_DIR)/CLAUDE.md
	@echo $(CLAUDE_DIR)/CLAUDE.md >> $(MANIFEST)
	@for f in $(RULE_FILES); do \
		dest=$(RULES_DIR)/$$(basename $$f); \
		cp $$f $$dest; \
		echo $$dest >> $(MANIFEST); \
	done
	cp target/release/workslate $(BIN_DIR)/workslate
	@echo $(BIN_DIR)/workslate >> $(MANIFEST)
	@if command -v claude >/dev/null 2>&1; then \
		echo "Registering workslate MCP server..."; \
		claude mcp add workslate -s user --transport stdio -- workslate 2>/dev/null && \
			echo "  MCP server registered." || \
			echo "  MCP registration failed. Run manually: claude mcp add workslate -s user --transport stdio -- workslate"; \
	else \
		echo "Claude Code CLI not found. Register MCP server manually:"; \
		echo "  claude mcp add workslate -s user --transport stdio -- workslate"; \
	fi
	@echo "Installed to $(CLAUDE_DIR) and $(BIN_DIR)/workslate"
	@echo "Manifest: $(MANIFEST)"

uninstall:
	@if [ ! -f $(MANIFEST) ]; then \
		echo "No manifest found at $(MANIFEST). Nothing to uninstall."; \
		exit 0; \
	fi
	@while IFS= read -r f; do \
		if [ -f "$$f" ]; then \
			case "$$f" in \
				*.md) \
					if head -1 "$$f" | grep -q '$(SIGNATURE)'; then \
						rm -f "$$f"; \
						echo "  removed $$f"; \
					else \
						echo "  skipped $$f (signature mismatch)"; \
					fi ;; \
				*) \
					rm -f "$$f"; \
					echo "  removed $$f" ;; \
			esac; \
		fi; \
	done < $(MANIFEST)
	rm -f $(MANIFEST)
	@if command -v claude >/dev/null 2>&1; then \
		claude mcp remove workslate -s user 2>/dev/null && echo "  MCP server unregistered." || true; \
	fi
	@echo "Uninstalled"
