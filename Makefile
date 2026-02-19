export PATH := /opt/homebrew/opt/llvm/bin:$(HOME)/.cargo/bin:$(PATH)
export LLVM_COV ?= $(shell command -v llvm-cov 2>/dev/null || echo /opt/homebrew/opt/llvm/bin/llvm-cov)
export LLVM_PROFDATA ?= $(shell command -v llvm-profdata 2>/dev/null || echo /opt/homebrew/opt/llvm/bin/llvm-profdata)
COVERAGE_BUILD_JOBS ?= 4

.PHONY: build test lint fmt fmt-check coverage check dev setup

build:
	cargo build

test:
	cargo test --all

lint:
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

coverage:
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { \
		echo "Error: cargo-llvm-cov is required for coverage checks."; \
		echo "Install with: cargo install cargo-llvm-cov"; \
		exit 1; \
	}
	CARGO_BUILD_JOBS=$(COVERAGE_BUILD_JOBS) cargo llvm-cov --all --fail-under-lines 80

check: fmt-check lint test coverage

dev:
	./scripts/dev.sh

setup:
	./scripts/setup.sh

code-stats:
	cloc . --exclude-dir=target,.venv,.git,_local,.pytest_cache,.ruff_cache,.vscode,uv.lock,.mypy_cache,.local,tests --by-file-by-lang 


