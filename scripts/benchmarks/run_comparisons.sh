#!/usr/bin/env sh

###
# Benchmark Ruff's performance against a variety of similar tools.
###

hyperfine --ignore-failure --warmup 2 --runs 5\
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --exit-zero" \
  "pyflakes crates/ruff_linter/resources/test/cpython" \
  "autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys crates/ruff_linter/resources/test/cpython" \
  "pycodestyle crates/ruff_linter/resources/test/cpython" \
  "flake8 crates/ruff_linter/resources/test/cpython --exit-zero"
