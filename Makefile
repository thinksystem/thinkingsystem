# Rust Project Makefile
.PHONY: help fmt fmt-check lint test audit deny check ci clean build release doc doc-ci tools fix

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

fmt: ## Format code with rustfmt
	cargo fmt --all

fmt-check: ## Check code formatting
	cargo fmt --all -- --check

lint: ## Run clippy lints
	cargo clippy --workspace --all-targets --all-features -- -D warnings

test: ## Run tests
	cargo test --workspace --all-features

audit: ## Run security audit
	cargo audit

deny: ## Run cargo deny checks
	cargo deny check

check: fmt-check lint test ## Run all checks (formatting, linting, tests)

ci: tools fmt-check lint test audit deny ## CI pipeline checks

clean: ## Clean build artefacts
	cargo clean

clean-all: ## Deep clean: Rust targets and Elixir build deps (keeps models/ by default)
	# Clean Rust workspace targets (root and nested)
	cargo clean
	# Clean Elixir native fabric artefacts
	rm -rf bin/gtr-fabric/_build bin/gtr-fabric/deps
	# Remove nested Rust target directories, tmp, and common caches
	find . -type d -name target -prune -exec rm -rf {} +
	find . -type d -name tmp -prune -exec rm -rf {} +
	find . -type d -name .pytest_cache -prune -exec rm -rf {} +
	find . -type d -name .mypy_cache -prune -exec rm -rf {} +

clean-models: ## Remove downloaded/committed local models (danger: large downloads later)
	rm -rf models/

build: ## Build project
	cargo build --workspace

release: ## Build optimised release
	cargo build --workspace --release

doc: ## Generate documentation (opens in browser)
	cargo doc --workspace --no-deps --open

doc-ci: ## Generate documentation (CI-safe, no open)
	cargo doc --workspace --no-deps

tools: ## Install cargo-audit and cargo-deny if missing
	@command -v cargo-audit >/dev/null 2>&1 || cargo install cargo-audit --locked
	@command -v cargo-deny >/dev/null 2>&1 || cargo install cargo-deny --locked

fix: ## Apply suggested fixes with cargo fix
	cargo fix --workspace --allow-dirty --allow-staged

git-clean: ## Remove all untracked files/dirs ignored by .gitignore (DANGER: irreversible)
	git status >/dev/null 2>&1 || true
	git clean -fdX

git-clean-dry: ## Preview what git-clean would remove (ignored files only)
	git clean -fdnX
