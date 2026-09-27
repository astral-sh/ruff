#!/usr/bin/env bash

rustup default < rust-toolchain
rustup component add clippy rustfmt
cargo install --locked cargo-insta@1.48.0
cargo fetch

pip install maturin prek
