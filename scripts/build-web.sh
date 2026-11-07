#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --offline --release --lib --target wasm32-unknown-unknown
python3 scripts/package.py web
