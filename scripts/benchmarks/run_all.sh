#!/usr/bin/env sh

###
# Benchmark Ruff's performance against a variety of similar tools, suppressing output as much as
# possible (so as to reduce I/O overhead).
###

# Note: Flake8's `checker.py` requires the following variant of `mp_run`:
#   def _mp_run(filename: str) -> tuple[str, Results, dict[str, int]]:
#       try:
#           return FileChecker(
#               filename=filename, plugins=_mp_plugins, options=_mp_options
#           ).run_checks()
#       except:
#           return (filename, [], {
#               "files": 0,
#               "logical lines": 0,
#               "physical lines": 0,
#               "tokens": 0,
#           })

hyperfine --ignore-failure --warmup 5 \
  "./target/release/ruff ./crates/ruff_linter/resources/test/cpython/ --no-cache --silent --select ALL" \
  "flake8 crates/ruff_linter/resources/test/cpython -qq --docstring-convention=all" \
  "pycodestyle crates/ruff_linter/resources/test/cpython -qq" \
  "pylint crates/ruff_linter/resources/test/cpython -j 0  --recursive=y --disable=E,W,C,R"
