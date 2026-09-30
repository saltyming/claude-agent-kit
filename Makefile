# Makefile for claude-agent-kit: runs install.sh against the payload in dist/.
#
#   make install                 full install or reinstall
#   make configure               prefs, custom rules, native configuration, servers
#   make uninstall               reverse what install recorded
#   make install ARGS="--binaries build --slate-dir ../slate-agent-kit --yes"
#
# ARGS is passed to slate-setup unchanged; see `sh install.sh --help`.
ARGS ?=

.DEFAULT_GOAL := help
.PHONY: help install configure uninstall

help: ## Show this list
	@echo "claude-agent-kit"
	@echo ""
	@echo "  make install     Full install or reinstall from dist/"
	@echo "  make configure   Prefs, custom rules, native configuration and server registration"
	@echo "  make uninstall   Reverse what install recorded"
	@echo "  make help        Show this list"
	@echo ""
	@echo "Pass installer options with ARGS, for example: make install ARGS=\"--yes --dry-run\""

install: ## Full install or reinstall from dist/
	@sh ./install.sh install $(ARGS)

configure: ## Prefs, custom rules, native configuration and server registration
	@sh ./install.sh configure $(ARGS)

uninstall: ## Reverse what install recorded
	@sh ./install.sh uninstall $(ARGS)
