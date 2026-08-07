#!/bin/bash
# Install ltop - btop-inspired TUI for llama.cpp
set -e

cd "$(dirname "$0")"
cargo build --release
echo ""
echo "✓ ltop built successfully!"
echo ""
echo "Run with:    ./target/release/ltop"
echo "Or install:  sudo cp target/release/ltop /usr/local/bin/ltop"
echo "Then run:    ltop"
