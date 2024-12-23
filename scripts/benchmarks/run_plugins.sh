#!/usr/bin/env sh

###
# Benchmark the incremental performance of each subsequent plugin.
###

cargo build --release && hyperfine --warmup 10 \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select C90 --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select I --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select D --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select UP --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select N --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select YTT --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select ANN --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select S --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select BLE --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select FBT --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select B --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select A --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select C4 --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select T10 --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select EM --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select ISC --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select ICN --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select T20 --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select PT --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select Q --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select RET --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select SIM --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select TID --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select ARG --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select DTZ --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select ERA --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select PD --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select PGH --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select PLC --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select PLE --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select PLR --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select PLW --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select PIE --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select COM --exit-zero" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --extend-select RUF --exit-zero"
