#!/usr/bin/env bash

rustup default < rust-toolchain
rustup component add clippy rustfmt
cargo install cargo-insta
cargo fetch

pip install maturin pre-commit

# configure git to be able to run from the devcontainer
git config --global --add safe.directory /workspaces/ruff

# ensure pre-commit is configured to run
pre-commit install
