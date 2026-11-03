#!/usr/bin/env sh

###
# Benchmark the Ruff formatter's performance against a variety of similar tools.
#
# Expects to be run from the repo root after invoking `cargo build --release`,
# in an environment with `black`, `autopep8`, and `yapf` installed from
# `scripts/benchmarks/uv.lock`.
#
# Example usage:
#
#   ./scripts/benchmarks/run_formatter.sh ~/workspace/zulip
###

TARGET_DIR=${1}

# In each case, ensure that we format the code in-place before invoking a given tool. This ensures
# a fair comparison across tools, since every tool is then running on a repository that already
# matches that tool's desired formatting.
#
# For example, if we're benchmarking Black's preview style, we first run `black --preview` over the
# target directory, thus ensuring that we're benchmarking preview style against a codebase that
# already conforms to it. The same goes for yapf, autoepp8, etc.

# Benchmark 1: Write to disk.
hyperfine --ignore-failure \
  --prepare "./target/release/ruff format ${TARGET_DIR}" \
  "./target/release/ruff format ${TARGET_DIR}" \
  --prepare "black --no-cache ${TARGET_DIR} --safe" \
  "black --no-cache ${TARGET_DIR} --safe" \
  --prepare "black --no-cache ${TARGET_DIR} --fast" \
  "black --no-cache ${TARGET_DIR} --fast" \
  --prepare "black --no-cache ${TARGET_DIR} --safe --preview" \
  "black --no-cache ${TARGET_DIR} --safe --preview" \
  --prepare "black --no-cache ${TARGET_DIR} --fast --preview" \
  "black --no-cache ${TARGET_DIR} --fast --preview" \
  --prepare "autopep8 ${TARGET_DIR} --recursive --in-place" \
  "autopep8 ${TARGET_DIR} --recursive --in-place" \
  --prepare "yapf ${TARGET_DIR} --parallel --recursive --in-place" \
  "yapf ${TARGET_DIR} --parallel --recursive --in-place"

# Benchmark 2: Write to disk, but only use one thread.
hyperfine --ignore-failure --env RAYON_NUM_THREADS=1 \
  --prepare "./target/release/ruff format ${TARGET_DIR}" \
  "./target/release/ruff format ${TARGET_DIR}" \
  --prepare "black --no-cache ${TARGET_DIR} --safe" \
  "black --no-cache ${TARGET_DIR} --workers=1 --safe" \
  --prepare "black --no-cache ${TARGET_DIR} --fast" \
  "black --no-cache ${TARGET_DIR} --workers=1 --fast" \
  --prepare "black --no-cache ${TARGET_DIR} --safe --preview" \
  "black --no-cache ${TARGET_DIR} --workers=1 --safe --preview" \
  --prepare "black --no-cache ${TARGET_DIR} --fast --preview" \
  "black --no-cache ${TARGET_DIR} --workers=1 --fast --preview" \
  --prepare "autopep8 ${TARGET_DIR} --recursive --in-place" \
  "autopep8 ${TARGET_DIR} --in-place --recursive --jobs=1" \
  --prepare "yapf ${TARGET_DIR} --parallel --recursive --in-place" \
  "yapf ${TARGET_DIR} --recursive --in-place"

# Benchmark 3: Check formatting, but don't write to disk.
hyperfine --ignore-failure \
  --prepare "./target/release/ruff format ${TARGET_DIR}" \
  "./target/release/ruff format ${TARGET_DIR} --check" \
  --prepare "black --no-cache ${TARGET_DIR} --safe" \
  "black --no-cache ${TARGET_DIR} --check --safe" \
  --prepare "black --no-cache ${TARGET_DIR} --fast" \
  "black --no-cache ${TARGET_DIR} --check --fast" \
  --prepare "black --no-cache ${TARGET_DIR} --safe --preview" \
  "black --no-cache ${TARGET_DIR} --check --safe --preview" \
  --prepare "black --no-cache ${TARGET_DIR} --fast --preview" \
  "black --no-cache ${TARGET_DIR} --check --fast --preview" \
  --prepare "autopep8 ${TARGET_DIR} --recursive --in-place" \
  "autopep8 ${TARGET_DIR} --recursive --diff" \
  --prepare "yapf ${TARGET_DIR} --parallel --recursive --in-place" \
  "yapf ${TARGET_DIR} --parallel --recursive --quiet"
