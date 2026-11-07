#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --offline --release --bin ground
python3 scripts/package.py native
