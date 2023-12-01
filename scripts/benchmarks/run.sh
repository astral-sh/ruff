#!/usr/bin/env sh

###
# Benchmark Ruff on the CPython codebase.
###

cargo build --release && hyperfine --ignore-failure --warmup 10 \
  "./target/release/ruff ./crates/ruff_linter/resources/test/cpython/ --no-cache"
