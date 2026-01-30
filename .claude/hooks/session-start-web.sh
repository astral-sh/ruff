#!/bin/bash
set -euo pipefail

# Install `gh`
if ! command -v gh &> /dev/null; then
    apt-get update -qq
    apt-get install -y -qq gh
fi

# Install clippy and rustfmt for the active toolchain.
rustup component add clippy rustfmt

# Our CLAUDE.md says to use nextest, but it's slow to install, so just tell
# CCfW not to try to use it.
echo "nextest is not installed, so just use 'cargo test' instead."
