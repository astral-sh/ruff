#!/usr/bin/env bash
# **NOTE**
# This script is being replaced by the ruff-ecosystem package which is no
# longer focused on black-compatibility but on changes in formatting between
# ruff versions. ruff-ecosystem does not support instability checks yet.
#
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
git -C "$dir/twine" checkout -q ae71822a3cb0478d0f6a0cccb65d6f8e6275ece5

# web framework that implements a lot of magic
if [ ! -d "$dir/django/.git" ]; then
  git clone --filter=tree:0 https://github.com/django/django "$dir/django"
fi
git -C "$dir/django" checkout -q ee5147cfd7de2add74a285537a8968ec074e70cd

# an ML project
if [ ! -d "$dir/transformers/.git" ]; then
  git clone --filter=tree:0 https://github.com/huggingface/transformers "$dir/transformers"
fi
git -C "$dir/transformers" checkout -q ac5a0556f14dec503b064d5802da1092e0b558ea

# type annotations
if [ ! -d "$dir/typeshed/.git" ]; then
  git clone --filter=tree:0 https://github.com/python/typeshed "$dir/typeshed"
fi
git -C "$dir/typeshed" checkout -q d34ef50754de993d01630883dbcd1d27ba507143

# python 3.11, typing and 100% test coverage
if [ ! -d "$dir/warehouse/.git" ]; then
  git clone --filter=tree:0 https://github.com/pypi/warehouse "$dir/warehouse"
fi
git -C "$dir/warehouse" checkout -q 5a4d2cadec641b5d6a6847d0127940e0f532f184

# zulip, a django user
if [ ! -d "$dir/zulip/.git" ]; then
  git clone --filter=tree:0 https://github.com/zulip/zulip "$dir/zulip"
fi
git -C "$dir/zulip" checkout -q ccddbba7a3074283ccaac3bde35fd32b19faf042

# home-assistant, home automation with 1ok files
if [ ! -d "$dir/home-assistant/.git" ]; then
  git clone --filter=tree:0 https://github.com/home-assistant/core "$dir/home-assistant"
fi
git -C "$dir/home-assistant" checkout -q 3601c531f400255d10b82529549e564fbe483a54

# poetry, a package manager that uses black preview style
if [ ! -d "$dir/poetry/.git" ]; then
  git clone --filter=tree:0 https://github.com/python-poetry/poetry "$dir/poetry"
fi
git -C "$dir/poetry" checkout -q 36fedb59b8e655252168055b536ead591068e1e4

# cpython itself
if [ ! -d "$dir/cpython/.git" ]; then
  git clone --filter=tree:0 https://github.com/python/cpython "$dir/cpython"
fi
git -C "$dir/cpython" checkout -q 28aea5d07d163105b42acd81c1651397ef95ea57

# Uncomment if you want to update the hashes
#for i in "$dir"/*/; do git -C "$i" switch main && git -C "$i" pull; done
#for i in "$dir"/*/; do echo "# $(basename "$i") $(git -C "$i" rev-parse HEAD)"; done

time cargo run --bin ruff_dev -- format-dev --stability-check \
  --error-file "$target/progress_projects_errors.txt" --log-file "$target/progress_projects_log.txt" --stats-file "$target/progress_projects_stats.txt" \
  --files-with-errors 3 --multi-project "$dir" || (
  echo "Ecosystem check failed"
  cat "$target/progress_projects_log.txt"
  exit 1
)
cat "$target/progress_projects_stats.txt"
