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
git -C "$dir/django" checkout 95e4d6b81312fdd9f8ebf3385be1c1331168b5cf
# an ML project
if [ ! -d "$dir/transformers/.git" ]; then
  git clone --filter=tree:0 https://github.com/huggingface/transformers "$dir/transformers"
fi
git -C "$dir/django" checkout 95e4d6b81312fdd9f8ebf3385be1c1331168b5cf
# type annotations
if [ ! -d "$dir/typeshed/.git" ]; then
  git clone --filter=tree:0 https://github.com/python/typeshed "$dir/typeshed"
fi
git -C "$dir/typeshed" checkout 7d33060e6ae3ebe54462a891f0c566c97371915b
# python 3.11, typing and 100% test coverage
if [ ! -d "$dir/warehouse/.git" ]; then
  git clone --filter=tree:0 https://github.com/pypi/warehouse "$dir/warehouse"
fi
git -C "$dir/warehouse" checkout fe6455c0a946e81f61d72edc1049f536d8bba903
# zulip, a django user
if [ ! -d "$dir/zulip/.git" ]; then
  git clone --filter=tree:0 https://github.com/zulip/zulip "$dir/zulip"
fi
git -C "$dir/zulip" checkout 6cb080c4479546a7f5cb017fcddea56605910b48
# cpython itself
if [ ! -d "$dir/cpython/.git" ]; then
  git clone --filter=tree:0 https://github.com/python/cpython "$dir/cpython"
fi
git -C "$dir/cpython" checkout 45de31db9cc9be945702f3a7ca35bbb9f98476af

# Uncomment if you want to update the hashes
# for i in "$dir"/*/; do git -C "$i" switch main && git -C "$i" pull && echo "# $(basename "$i") $(git -C "$i" rev-parse HEAD)"; done

time cargo run --bin ruff_dev -- format-dev --stability-check \
  --error-file "$target/progress_projects_errors.txt" --log-file "$target/progress_projects_log.txt" --stats-file "$target/progress_projects_stats.txt" \
  --files-with-errors 25 --multi-project "$dir" || (
  echo "Ecosystem check failed"
  cat "$target/progress_projects_log.txt"
  exit 1
)
cat "$target/progress_projects_stats.txt"
