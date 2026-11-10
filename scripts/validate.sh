#!/usr/bin/env bash
# Standalone validation: Rust stable, a C compiler and Linux graphics runtime libraries.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-4}"
cargo test --workspace --locked --release
cargo build --locked --release -p brismaster
