#!/usr/bin/env bash
# Fixed seeds; elapsed time is descriptive, never a test assertion or strength rating.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
for spec in 'poker-core equity-bench' 'poker-it draw-bench' 'blackjack-core ev-bench' 'scala40-core solver-bench'; do
  read -r package example <<< "$spec"
  cargo run --locked --release -p "$package" --example "$example"
done
