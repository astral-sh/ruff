#!/usr/bin/env sh

###
# Benchmark the incremental performance of each subsequent plugin.
###

cargo build --release && hyperfine --ignore-failure --warmup 10 \
  "./target/release/ruff ./resources/test/cpython/ --no-cache" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select C90" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select I" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select D" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select UP" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select N" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select YTT" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select ANN" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select S" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select BLE" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select FBT" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select B" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select A" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select C4" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select T10" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select EM" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select ISC" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select ICN" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select T20" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select PT" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select Q" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select RET" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select SIM" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select TID" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select ARG" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select DTZ" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select ERA" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select PD" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select PGH" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select PLC" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select PLE" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select PLR" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select PLW" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select PIE" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select COM" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --extend-select RUF"
