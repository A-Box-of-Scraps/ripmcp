#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

cargo build --locked --release --bin ripmcp
mkdir -p dist
tar -czf dist/ripmcp-x86_64-unknown-linux-gnu.tar.gz -C target/release ripmcp
cd dist
sha256sum ripmcp-x86_64-unknown-linux-gnu.tar.gz > SHA256SUMS
