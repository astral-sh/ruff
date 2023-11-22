#!/bin/bash
# This is @konstin's setup for checking an entire checkout of ~3k packages for
# panics, fix errors and similar problems.
#
# We put this in a docker container because processing random scraped code from GitHub is
# [kinda dangerous](https://moyix.blogspot.com/2022/09/someones-been-messing-with-my-subnormals.html)
#
# Usage:
# ```shell
# # You can also use any other check_ecosystem.py input file
# curl https://raw.githubusercontent.com/akx/ruff-usage-aggregate/master/data/known-github-tomls-clean.jsonl > github_search.jsonl
# cargo build --release --target x86_64-unknown-linux-musl --bin ruff
# scripts/ecosystem_all_check.sh check --select RUF200
# ```

# https://stackoverflow.com/a/246128/3549270
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

time docker run --rm -it \
  -w /app \
  -v "${SCRIPT_DIR}/../target/checkouts:/app/checkouts" \
  -v "${SCRIPT_DIR}/../target/ecosystem_all_results:/app/ecosystem_all_results" \
  -v "${SCRIPT_DIR}/../target/x86_64-unknown-linux-musl/release/ruff:/app/ruff" \
  -v "${SCRIPT_DIR}/../ecosystem_all.py:/app/ecosystem_all.py" \
  -v "${SCRIPT_DIR}/../github_search.jsonl:/app/github_search.jsonl" \
  -v "${SCRIPT_DIR}/../.venv-3.11:/app/.venv" \
  -v "${SCRIPT_DIR}/ecosystem_all_check_entrypoint.sh:/app/ecosystem_all_check_entrypoint.sh" \
  -v "${SCRIPT_DIR}/ecosystem_all_check.py:/app/ecosystem_all_check.py" \
  python:3.11 ./ecosystem_all_check_entrypoint.sh "$@"

# grep the fix errors
grep -R "the rule codes" "${SCRIPT_DIR}/../target/ecosystem_all_results" | sort > "${SCRIPT_DIR}/../target/fix-errors.txt"
# Make sure we didn't have an early exit
echo "Done"
