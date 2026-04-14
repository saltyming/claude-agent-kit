CLAUDE_DIR := $(HOME)/.claude
RULES_DIR  := $(CLAUDE_DIR)/rules
BIN_DIR    := $(HOME)/.local/bin
MANIFEST   := $(CLAUDE_DIR)/.claude-agent-kit-manifest
SIGNATURE        := claude-agent-kit
CUSTOM_SIGNATURE := claude-agent-kit-custom

RULE_FILES := $(wildcard claude-rules/*.md)

PREFS_TEMPLATE  := scripts/claude-agent-kit--aside-prefs.md.tmpl
CONFIGURE_ASIDE := scripts/configure-aside.sh

.PHONY: install uninstall build configure

build:
	cargo build --release -p workslate -p aside

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
	@# Install binaries
	@for bin in workslate aside; do \
		cp target/release/$$bin $(BIN_DIR)/$$bin; \
		if [ "$$(uname -s)" = "Darwin" ] && command -v codesign >/dev/null 2>&1; then \
			codesign --force --sign - $(BIN_DIR)/$$bin 2>/dev/null && \
				echo "  Code signed (ad-hoc): $$bin." || true; \
		fi; \
		echo $(BIN_DIR)/$$bin >> $(MANIFEST); \
	done
	@# Register both MCP servers
	@if command -v claude >/dev/null 2>&1; then \
		for srv in workslate aside; do \
			echo "Registering $$srv MCP server..."; \
			claude mcp add $$srv -s user --transport stdio -- $$srv 2>/dev/null && \
				echo "  $$srv registered." || \
				echo "  $$srv registration failed. Run manually: claude mcp add $$srv -s user --transport stdio -- $$srv"; \
		done; \
	else \
		echo "Claude Code CLI not found. Register MCP servers manually:"; \
		echo "  claude mcp add workslate -s user --transport stdio -- workslate"; \
		echo "  claude mcp add aside -s user --transport stdio -- aside"; \
	fi
	@# Interactive aside configuration + optional custom rule ingestion
	@CLAUDE_DIR=$(CLAUDE_DIR) RULES_DIR=$(RULES_DIR) MANIFEST=$(MANIFEST) \
		TEMPLATE_SRC=$(PREFS_TEMPLATE) \
		sh $(CONFIGURE_ASIDE)
	@echo ""
	@echo "Installed to $(CLAUDE_DIR) and $(BIN_DIR)/{workslate,aside}"
	@echo "Manifest: $(MANIFEST)"

configure:
	@mkdir -p $(RULES_DIR)
	@[ -f $(MANIFEST) ] || : > $(MANIFEST)
	@CLAUDE_DIR=$(CLAUDE_DIR) RULES_DIR=$(RULES_DIR) MANIFEST=$(MANIFEST) \
		TEMPLATE_SRC=$(PREFS_TEMPLATE) \
		ASIDE_RECONFIGURE=yes \
		sh $(CONFIGURE_ASIDE)

uninstall:
	@if [ ! -f $(MANIFEST) ]; then \
		echo "No manifest found at $(MANIFEST). Nothing to uninstall."; \
		exit 0; \
	fi
	@# First pass: remove core-signed files; collect custom-signed ones.
	@custom_list=""; \
	while IFS= read -r f; do \
		if [ -f "$$f" ]; then \
			case "$$f" in \
				*.md) \
					first="$$(head -1 "$$f" 2>/dev/null || true)"; \
					if printf '%s' "$$first" | grep -Fq "<!-- $(CUSTOM_SIGNATURE)"; then \
						custom_list="$$custom_list$$f\n"; \
					elif printf '%s' "$$first" | grep -Fq "<!-- $(SIGNATURE) -->"; then \
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
	done < $(MANIFEST); \
	\
	if [ -n "$$custom_list" ]; then \
		echo ""; \
		echo "The following user-owned files were installed alongside the kit:"; \
		printf "$$custom_list" | sed 's/^/  /'; \
		keep="yes"; \
		if [ -n "$$ASIDE_UNINSTALL_KEEP_PREFS" ]; then \
			case "$$ASIDE_UNINSTALL_KEEP_PREFS" in \
				no|NO|No|n|N) keep="no" ;; \
				*) keep="yes" ;; \
			esac; \
		elif [ -r /dev/tty ]; then \
			printf "Remove these too? [y/N]: " > /dev/tty; \
			read answer < /dev/tty || answer=""; \
			case "$$answer" in \
				y|Y|yes|YES|Yes) keep="no" ;; \
				*) keep="yes" ;; \
			esac; \
		fi; \
		if [ "$$keep" = "no" ]; then \
			printf "$$custom_list" | while IFS= read -r f; do \
				[ -z "$$f" ] && continue; \
				rm -f "$$f" && echo "  removed $$f"; \
			done; \
		else \
			echo ""; \
			echo "Preserved (not managed by claude-agent-kit from this point on):"; \
			printf "$$custom_list" | sed 's/^/  /'; \
			echo "Remove manually with:  rm $$(printf "$$custom_list" | tr '\n' ' ')"; \
		fi; \
	fi
	@rm -f $(MANIFEST)
	@if command -v claude >/dev/null 2>&1; then \
		for srv in workslate aside; do \
			claude mcp remove $$srv -s user 2>/dev/null && echo "  $$srv unregistered." || true; \
		done; \
	fi
	@echo "Uninstalled"
