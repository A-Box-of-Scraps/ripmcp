#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update && sudo apt-get install -y groff man-db
curl -fL https://github.com/jgm/pandoc/releases/download/3.11/pandoc-3.11-linux-amd64.tar.gz -o "$RUNNER_TEMP/pandoc.tar.gz"
echo "37edb3bbcf722f921a009941bf5874e2e0c09263226c9b4a2d980788cb062ab6  $RUNNER_TEMP/pandoc.tar.gz" | sha256sum -c -
tar -xzf "$RUNNER_TEMP/pandoc.tar.gz" -C "$RUNNER_TEMP"
echo "$RUNNER_TEMP/pandoc-3.11/bin" >> "$GITHUB_PATH"
