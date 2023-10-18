#!/usr/bin/env bash
# Check black compatibility and check for formatter instabilities and other
# errors.
#
# This script will first clone a diverse set of (mostly) black formatted
# repositories with fixed revisions to target/progress_projects. Each project
# gets formatted (without modifying the files on disk) to check how
# similar our style is to black. It also catches common issues such as
# unstable formatting, internal formatter errors and printing invalid syntax.
#
# The pinned revisions are the latest of this writing, update freely.

set -e

target=$(git rev-parse --show-toplevel)/target
dir="$target/progress_projects"
mkdir -p "$dir"

# small util library
if [ ! -d "$dir/twine/.git" ]; then
  git clone --filter=tree:0 https://github.com/pypa/twine "$dir/twine"
fi
git -C "$dir/twine" checkout -q afc37f8b26ed06ccd104f6724f293f657b9b7f15

# web framework that implements a lot of magic
if [ ! -d "$dir/django/.git" ]; then
  git clone --filter=tree:0 https://github.com/django/django "$dir/django"
fi
git -C "$dir/django" checkout -q 20b7aac7ca60b0352d926340622e618bcbee54a8

# an ML project
if [ ! -d "$dir/transformers/.git" ]; then
  git clone --filter=tree:0 https://github.com/huggingface/transformers "$dir/transformers"
fi
git -C "$dir/transformers" checkout -q 5c081e29930466ecf9a478727039d980131076d9

# type annotations
if [ ! -d "$dir/typeshed/.git" ]; then
  git clone --filter=tree:0 https://github.com/python/typeshed "$dir/typeshed"
fi
git -C "$dir/typeshed" checkout -q cb688d2577520d98c09853acc20de099300b4e48

# python 3.11, typing and 100% test coverage
if [ ! -d "$dir/warehouse/.git" ]; then
  git clone --filter=tree:0 https://github.com/pypi/warehouse "$dir/warehouse"
fi
git -C "$dir/warehouse" checkout -q c6d9dd32b7c85d3a5f4240c95267874417e5b965

# zulip, a django user
if [ ! -d "$dir/zulip/.git" ]; then
  git clone --filter=tree:0 https://github.com/zulip/zulip "$dir/zulip"
fi
git -C "$dir/zulip" checkout -q b605042312c763c9a1e458f0ca6a003799682546

# home-assistant, home automation with 1ok files
if [ ! -d "$dir/home-assistant/.git" ]; then
  git clone --filter=tree:0 https://github.com/home-assistant/core "$dir/home-assistant"
fi
git -C "$dir/home-assistant" checkout -q 88296c1998fd1943576e0167ab190d25af175257

# poetry, a package manager that uses black preview style
if [ ! -d "$dir/poetry/.git" ]; then
  git clone --filter=tree:0 https://github.com/python-poetry/poetry "$dir/poetry"
fi
git -C "$dir/poetry" checkout -q f5cb9f0fb19063cf280faf5e39c82d5691da9939

# cpython itself
if [ ! -d "$dir/cpython/.git" ]; then
  git clone --filter=tree:0 https://github.com/python/cpython "$dir/cpython"
fi
git -C "$dir/cpython" checkout -q b75186f69edcf54615910a5cd707996144163ef7

# poetry itself
if [ ! -d "$dir/poetry/.git" ]; then
  git clone --filter=tree:0 https://github.com/python-poetry/poetry "$dir/poetry"
fi
git -C "$dir/poetry" checkout -q 611033a7335f3c8e2b74dd58688fb9021cf84a5b

# Uncomment if you want to update the hashes
#for i in "$dir"/*/; do git -C "$i" switch main && git -C "$i" pull; done
#for i in "$dir"/*/; do echo "# $(basename "$i") $(git -C "$i" rev-parse HEAD)"; done

time cargo run --bin ruff_dev -- format-dev --stability-check \
  --error-file "$target/progress_projects_errors.txt" --log-file "$target/progress_projects_log.txt" --stats-file "$target/progress_projects_stats.txt" \
  --files-with-errors 14 --multi-project "$dir" || (
  echo "Ecosystem check failed"
  cat "$target/progress_projects_log.txt"
  exit 1
)
cat "$target/progress_projects_stats.txt"
