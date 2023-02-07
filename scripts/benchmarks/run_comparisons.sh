#!/usr/bin/env sh

###
# Benchmark Ruff's performance against a variety of similar tools.
###

hyperfine --ignore-failure --warmup 5 \
  "./target/release/ruff ./crates/ruff/resources/test/cpython/ --no-cache" \
  "pyflakes crates/ruff/resources/test/cpython" \
  "autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys crates/ruff/resources/test/cpython" \
  "pycodestyle crates/ruff/resources/test/cpython" \
  "flake8 crates/ruff/resources/test/cpython"
