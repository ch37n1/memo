#!/usr/bin/env bash
set -euo pipefail

# Check required tools
if ! command -v cargo &>/dev/null; then
    echo "error: cargo not found. Install Rust toolchain first." >&2
    exit 1
fi

if ! command -v pre-commit &>/dev/null; then
    echo "error: pre-commit not found. Install via 'pip install pre-commit' or 'brew install pre-commit'" >&2
    exit 1
fi

# Ensure LLVM coverage tools are available
if ! command -v llvm-cov &>/dev/null || ! command -v llvm-profdata &>/dev/null; then
    if command -v rustup &>/dev/null; then
        echo "Installing llvm-tools-preview component via rustup..."
        rustup component add llvm-tools-preview
    else
        echo "error: llvm-cov and llvm-profdata are required for coverage checks." >&2
        echo "Install LLVM via Homebrew: brew install llvm" >&2
        echo "Then add to PATH: export PATH=\"/opt/homebrew/opt/llvm/bin:\$PATH\"" >&2
        exit 1
    fi
fi

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
