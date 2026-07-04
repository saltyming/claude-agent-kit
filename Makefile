CLAUDE_DIR ?= $(HOME)/.claude
RULES_DIR  := $(CLAUDE_DIR)/rules
SKILLS_DIR := $(CLAUDE_DIR)/skills
BIN_DIR    ?= $(HOME)/.local/bin
SKIP_MCP   ?= 0
MANIFEST   := $(CLAUDE_DIR)/.claude-agent-kit-manifest
SIGNATURE        := claude-agent-kit
CUSTOM_SIGNATURE := claude-agent-kit-custom

RULE_FILES := $(wildcard claude-rules/*.md)
SKILL_DIRS := $(wildcard claude-skills/palette-*)

CONFIGURE_PREFS    := scripts/configure-prefs.sh
CAK_COMMON         := scripts/cak-common.sh

.DEFAULT_GOAL := help
.PHONY: help install uninstall build configure install-mcp

help:
	@echo "claude-agent-kit — targets:"
	@echo "  install      build + install rules/skills/workslate, register MCP, configure prefs"
	@echo "  configure    re-run interactive aside/dispatch preference setup"
	@echo "  uninstall    remove kit-signed files (user-owned '-custom:' prefs are kept)"
	@echo "  build        compile the workslate binary only"
	@echo "  install-mcp  build/register the shared aside/dispatch servers via slate"
	@echo "Vars: SKIP_MCP=1  SLATE_AGENT_KIT_DIR=<path>  CLAUDE_DIR=<path>  BIN_DIR=<path>"

build:
	cargo build --release -p workslate

install: build
	@mkdir -p $(RULES_DIR) $(BIN_DIR) $(SKILLS_DIR)
	@: > $(MANIFEST)
	@if [ -f "$(CLAUDE_DIR)/CLAUDE.md" ] && ! head -1 "$(CLAUDE_DIR)/CLAUDE.md" | grep -Eq "<!-- (slate-agent-kit:common|$(SIGNATURE)) -->"; then \
		bak="$(CLAUDE_DIR)/CLAUDE.md.bak-$$(date -u +%Y%m%dT%H%M%SZ)"; \
		cp -p "$(CLAUDE_DIR)/CLAUDE.md" "$$bak"; \
		echo "  WARNING: existing $(CLAUDE_DIR)/CLAUDE.md is not managed by this kit; backed up to $$bak"; \
		echo "## backup: $$bak" >> $(MANIFEST); \
	fi
	cp CLAUDE.md $(CLAUDE_DIR)/CLAUDE.md
	@echo $(CLAUDE_DIR)/CLAUDE.md >> $(MANIFEST)
	@for f in $(RULE_FILES); do \
		dest=$(RULES_DIR)/$$(basename $$f); \
		cp $$f $$dest; \
		echo $$dest >> $(MANIFEST); \
	done
	@# Install palette skills (directory-shaped; record dest dir in the manifest)
	@for d in $(SKILL_DIRS); do \
		name=$$(basename $$d); \
		dest=$(SKILLS_DIR)/$$name; \
		rm -rf "$$dest"; \
		cp -R "$$d" "$$dest"; \
		echo $$dest >> $(MANIFEST); \
	done
	@# Install the workslate binary (aside/dispatch are shared — see install-mcp)
	@for bin in workslate; do \
		cp target/release/$$bin $(BIN_DIR)/$$bin.tmp.$$$$ && mv -f $(BIN_DIR)/$$bin.tmp.$$$$ $(BIN_DIR)/$$bin || cp target/release/$$bin $(BIN_DIR)/$$bin; \
		if [ "$$(uname -s)" = "Darwin" ] && command -v codesign >/dev/null 2>&1; then \
			codesign --force --sign - $(BIN_DIR)/$$bin 2>/dev/null && \
				echo "  Code signed (ad-hoc): $$bin." || echo "  WARNING: codesign failed for $$bin; macOS may SIGKILL the unsigned binary." >&2; \
		fi; \
		echo $(BIN_DIR)/$$bin >> $(MANIFEST); \
	done
	@# Register PreToolUse doorbell hooks in settings.json
	@$(BIN_DIR)/workslate --install-hooks || echo "  Hook registration failed. Run manually: $(BIN_DIR)/workslate --install-hooks"
	@# Register the workslate MCP server (Claude-only)
	@if command -v claude >/dev/null 2>&1; then \
		echo "Registering workslate MCP server..."; \
		claude mcp add workslate -s user --transport stdio -- $(BIN_DIR)/workslate 2>/dev/null && \
			echo "  workslate registered." || \
			echo "  workslate registration failed. Run manually: claude mcp add workslate -s user --transport stdio -- $(BIN_DIR)/workslate"; \
	else \
		echo "Claude Code CLI not found. Register manually:"; \
		echo "  claude mcp add workslate -s user --transport stdio -- $(BIN_DIR)/workslate"; \
	fi
	@# Build + register the SHARED aside/dispatch servers from slate-agent-kit
	@$(MAKE) --no-print-directory install-mcp
	@# Interactive aside + dispatch prefs (the shared configure-prefs.sh)
	@RULES_DIR=$(RULES_DIR) PREFIX=claude-agent-kit MANIFEST=$(MANIFEST) \
		sh $(CONFIGURE_PREFS)
	@# Shared custom-rules ingestion (once, via the cak-common.sh function)
	@RULES_DIR=$(RULES_DIR) MANIFEST=$(MANIFEST) \
		sh -c '. $(CAK_COMMON); ingest_custom_rules'
	@echo ""
	@echo "Installed to $(CLAUDE_DIR) and $(BIN_DIR)/{workslate,aside,dispatch}"
	@echo "Manifest: $(MANIFEST)"

install-mcp:
	@if [ "$(SKIP_MCP)" = "1" ]; then \
		echo "Skipping shared aside/dispatch registration because SKIP_MCP=1."; \
	else \
		slate_dir=""; \
		if [ -n "$${SLATE_AGENT_KIT_DIR:-}" ] && [ -x "$${SLATE_AGENT_KIT_DIR}/tooling/install-mcp.sh" ]; then slate_dir="$$SLATE_AGENT_KIT_DIR"; fi; \
		if [ -z "$$slate_dir" ] && [ -x "../slate-agent-kit/tooling/install-mcp.sh" ]; then slate_dir="../slate-agent-kit"; fi; \
		if [ -z "$$slate_dir" ] && [ -x "../../tooling/install-mcp.sh" ]; then slate_dir="../.."; fi; \
		if [ -z "$$slate_dir" ]; then \
			echo "slate-agent-kit not found — aside/dispatch (shared crates) were NOT installed."; \
			echo "Set SLATE_AGENT_KIT_DIR to a slate checkout and run 'make install-mcp',"; \
			echo "or run slate's tooling/install-mcp.sh --configure-claude directly."; \
			exit 1; \
		fi; \
		BIN_DIR="$(BIN_DIR)" CLAUDE_DIR="$(CLAUDE_DIR)" "$$slate_dir/tooling/install-mcp.sh" --configure-claude; \
	fi

configure:
	@mkdir -p $(RULES_DIR)
	@[ -f $(MANIFEST) ] || : > $(MANIFEST)
	@RULES_DIR=$(RULES_DIR) PREFIX=claude-agent-kit MANIFEST=$(MANIFEST) \
		PREFS_RECONFIGURE=yes \
		sh $(CONFIGURE_PREFS)
	@RULES_DIR=$(RULES_DIR) MANIFEST=$(MANIFEST) \
		sh -c '. $(CAK_COMMON); ingest_custom_rules'

uninstall:
	@if [ ! -f $(MANIFEST) ]; then \
		echo "No manifest found at $(MANIFEST). Nothing to uninstall."; \
		exit 0; \
	fi
	@# Remove PreToolUse doorbell hooks while the binary still exists
	@[ -x $(BIN_DIR)/workslate ] && $(BIN_DIR)/workslate --uninstall-hooks 2>/dev/null || true
	@# First pass: remove core-signed files; collect custom-signed ones.
	@custom_list=""; \
	while IFS= read -r f; do \
		if [ -f "$$f" ]; then \
			case "$$f" in \
				*.md) \
					first="$$(head -1 "$$f" 2>/dev/null || true)"; \
					if printf '%s' "$$first" | grep -Fq "<!-- $(CUSTOM_SIGNATURE)"; then \
						custom_list="$$custom_list$$f\n"; \
					elif printf '%s' "$$first" | grep -Eq "<!-- (slate-agent-kit:common|$(SIGNATURE)) -->"; then \
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
	@# Remove palette skill directories recorded in the manifest (core-signed only)
	@grep -E '/skills/palette-' $(MANIFEST) 2>/dev/null | while IFS= read -r d; do \
		if [ -d "$$d" ] && [ -f "$$d/SKILL.md" ] && grep -Eq "<!-- (slate-agent-kit:common|$(SIGNATURE)) -->" "$$d/SKILL.md"; then \
			rm -rf "$$d" && echo "  removed $$d"; \
		elif [ -e "$$d" ]; then \
			echo "  skipped $$d (signature mismatch)"; \
		fi; \
	done
	@rm -f $(MANIFEST)
	@if command -v claude >/dev/null 2>&1; then \
		for srv in workslate aside dispatch; do \
			claude mcp remove $$srv -s user 2>/dev/null && echo "  $$srv unregistered." || true; \
		done; \
	fi
	@echo "Note: the shared aside/dispatch binaries in $(BIN_DIR) are slate-owned; remove them via slate's tooling if desired."
	@echo "Uninstalled"
