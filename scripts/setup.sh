#!/usr/bin/env bash
set -euo pipefail

# Check required tools
if ! command -v cargo &>/dev/null; then
    echo "error: cargo not found. Install Rust via https://rustup.rs" >&2
    exit 1
fi

if ! command -v pre-commit &>/dev/null; then
    echo "error: pre-commit not found. Install via 'pip install pre-commit' or 'brew install pre-commit'" >&2
    exit 1
fi

# Install llvm-tools for cargo-llvm-cov
echo "Installing llvm-tools-preview component..."
rustup component add llvm-tools-preview

if ! cargo llvm-cov --version &>/dev/null 2>&1; then
    echo "Installing cargo-llvm-cov..."
    cargo install cargo-llvm-cov
fi

# Create XDG runtime directories
echo "Creating memo runtime directories..."
mkdir -p ~/.config/memo
mkdir -p ~/.local/share/memo
mkdir -p ~/.local/state/memo

echo "Installing pre-commit hooks..."
pre-commit install

echo "Setup complete."
