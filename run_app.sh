#!/bin/sh
# Build and start Thread Vision on macOS. Run from anywhere:  ./run_app.sh
# Rebuilds the release app, refreshes /Applications/Thread Vision.app from it and opens that.
# The app must run from its bundle: macOS only grants the bundle Local Network access to the
# GigE camera (see tools/mac/make_launcher.sh). pylon for macOS must be in /Library/Frameworks.
set -e
cd "$(dirname "$0")"
cargo build --release -p app
tools/mac/make_launcher.sh
open "/Applications/Thread Vision.app"
