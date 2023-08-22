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

set -ex

target=$(git rev-parse --show-toplevel)/target
dir="$target/progress_projects"
mkdir -p "$dir"

# small util library
if [ ! -d "$dir/twine/.git" ]; then
  git clone --filter=tree:0 https://github.com/pypa/twine "$dir/twine"
fi
git -C "$dir/twine" checkout 0bb428c410b8df64c04dc881ac1db37d932f3066

# web framework that implements a lot of magic
if [ ! -d "$dir/django/.git" ]; then
  git clone --filter=tree:0 https://github.com/django/django "$dir/django"
fi
git -C "$dir/django" checkout 48a1929ca050f1333927860ff561f6371706968a

# an ML project
if [ ! -d "$dir/transformers/.git" ]; then
  git clone --filter=tree:0 https://github.com/huggingface/transformers "$dir/transformers"
fi
git -C "$dir/transformers" checkout 62396cff46854dc53023236cfeb785993fa70067

# type annotations
if [ ! -d "$dir/typeshed/.git" ]; then
  git clone --filter=tree:0 https://github.com/python/typeshed "$dir/typeshed"
fi
git -C "$dir/typeshed" checkout 2c15a8e7906e19f49bb765e2807dd0079fe9c04b

# python 3.11, typing and 100% test coverage
if [ ! -d "$dir/warehouse/.git" ]; then
  git clone --filter=tree:0 https://github.com/pypi/warehouse "$dir/warehouse"
fi
git -C "$dir/warehouse" checkout 6be6bccf07dace18784ea8aeac7906903fdbcf3a

# zulip, a django user
if [ ! -d "$dir/zulip/.git" ]; then
  git clone --filter=tree:0 https://github.com/zulip/zulip "$dir/zulip"
fi
git -C "$dir/zulip" checkout 328cdde24331b82baa4c9b1bf1cb7b2015799826

# cpython itself
if [ ! -d "$dir/cpython/.git" ]; then
  git clone --filter=tree:0 https://github.com/python/cpython "$dir/cpython"
fi
git -C "$dir/cpython" checkout 1a1bfc28912a39b500c578e9f10a8a222638d411

# Uncomment if you want to update the hashes
#for i in "$dir"/*/; do git -C "$i" switch main && git -C "$i" pull; done
#for i in "$dir"/*/; do echo "# $(basename "$i") $(git -C "$i" rev-parse HEAD)"; done

time cargo run --bin ruff_dev -- format-dev --stability-check \
  --error-file "$target/progress_projects_errors.txt" --log-file "$target/progress_projects_log.txt" --stats-file "$target/progress_projects_stats.txt" \
  --files-with-errors 16 --multi-project "$dir" || (
  echo "Ecosystem check failed"
  cat "$target/progress_projects_log.txt"
  exit 1
)
cat "$target/progress_projects_stats.txt"
