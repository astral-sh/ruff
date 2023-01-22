#!/usr/bin/env sh

###
# Benchmark the incremental performance of each subsequent plugin.
###

cargo build --release && hyperfine --ignore-failure --warmup 10 \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select C90" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select I" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select D" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select UP" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select N" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select YTT" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select ANN" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select S" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select BLE" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select FBT" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select B" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select A" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select C4" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select T10" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select EM" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select ISC" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select ICN" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select T20" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select PT" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select Q" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select RET" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select SIM" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select TID" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select ARG" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select DTZ" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select ERA" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select PD" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select PGH" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select PLC" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select PLE" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select PLR" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select PLW" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select PIE" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select COM" \
  "./target/release/ruff ./resources/test/cpython/ --no-cache --silent --extend-select RUF"
