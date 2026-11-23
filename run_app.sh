#!/bin/sh
# Start Thread Vision on macOS. Run from the repository root:
#   ./run_app.sh
# (builds the release app first if needed). pylon for macOS must be installed in
# /Library/Frameworks; the rpath to it comes from .cargo/config.toml.
set -e
cd "$(dirname "$0")"
cargo build --release -p app
exec ./target/release/app
