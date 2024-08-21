#!/usr/bin/env bash

rustup default < rust-toolchain
rustup component add clippy rustfmt
cargo install cargo-insta
cargo fetch

pip install maturin pre-commit
