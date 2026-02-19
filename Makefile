export PATH := $(HOME)/.cargo/bin:$(PATH)

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
	cargo llvm-cov --all --fail-under-lines 80

check: fmt-check lint test coverage

dev:
	./scripts/dev.sh

setup:
	./scripts/setup.sh
