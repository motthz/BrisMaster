#!/usr/bin/env bash
# Compila Massima e il tavolo in WebAssembly e copia lab.wasm nella cartella del sito.
# Serve una volta: rustup target add wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release -p briscola-lab --lib --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/briscola_lab.wasm crates/lab/web/lab.wasm
echo "Pronto: crates/lab/web/ (prova in locale con: python3 -m http.server -d crates/lab/web 8000)"
