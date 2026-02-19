#!/usr/bin/env bash
set -euo pipefail

export RUST_LOG=debug

echo "Building memod (debug)..."
cargo build -p memod

echo "Starting memod..."
exec ./target/debug/memod
