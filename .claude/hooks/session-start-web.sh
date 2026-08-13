#!/bin/bash
set -euo pipefail

# Install `gh`
if ! command -v gh &> /dev/null; then
    apt-get update -qq
    apt-get install -y -qq gh
fi

# Set GH_REPO so `gh` works even when the git remote points to a local proxy
if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo 'export GH_REPO=astral-sh/ruff' >> "$CLAUDE_ENV_FILE"
fi

# Install clippy and rustfmt for the active toolchain.
rustup component add clippy rustfmt

# Our CLAUDE.md says to use nextest, but it's slow to install, so just tell
# CCfW not to try to use it.
echo "nextest is not installed, so just use 'cargo test' instead."
