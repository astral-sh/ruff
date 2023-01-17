#!/usr/bin/env sh

###
# Benchmark Ruff's performance against a variety of similar tools, suppressing output as much as
# possible (so as to reduce I/O overhead).
###

hyperfine --ignore-failure --warmup 5 \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent" \
  "pycodestyle resources/test/cpython -qq" \
  "flake8 resources/test/cpython -qq" \
  "pylint resources/test/cpython -j 0  --recursive=y --disable=E,W,C,R"
