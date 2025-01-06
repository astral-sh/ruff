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
# repositories with fixed revisions to target/formatter-ecosystem. Each project
# gets formatted (without modifying the files on disk) to check how
# similar our style is to black. It also catches common issues such as
# unstable formatting, internal formatter errors and printing invalid syntax.
#
# The pinned revisions are the latest of this writing, update freely.

set -e

target=$(git rev-parse --show-toplevel)/target
dir="$target/formatter-ecosystem"
mkdir -p "$dir"

# Perform an idempotent clone and checkout of a commit
clone_commit() {
    local repo="$1"
    local name="$2"
    local ref="$3"

    if [ -z "$repo" ] || [ -z "$name" ] || [ -z "$ref" ]; then
        echo "Usage: clone_commit <repo> <name> <ref>"
        return 1
    fi

    local target="$dir/projects/$name"

    if [ ! -d "$target/.git" ]; then
        echo "Cloning $repo to $name"
        # Perform a minimal clone, we only need a single commit
        git clone --filter=blob:none --depth=1 --no-tags --no-checkout --single-branch "$repo" "$target"
    fi

    echo "Using $repo at $ref"
    git -C "$target" fetch --filter=blob:none --depth=1 --no-tags origin "$ref"
    git -C "$target" checkout -q "$ref"
}

# small util library
clone_commit \
    "https://github.com/pypa/twine" \
    "twine" \
    "ae71822a3cb0478d0f6a0cccb65d6f8e6275ece5" &

# web framework that implements a lot of magic
clone_commit \
    "https://github.com/django/django" \
    "django" \
    "ee5147cfd7de2add74a285537a8968ec074e70cd" &

# an ML project
clone_commit \
    "https://github.com/huggingface/transformers" \
    "transformers" \
    "ac5a0556f14dec503b064d5802da1092e0b558ea" &

# type annotations
clone_commit \
    "https://github.com/python/typeshed" \
    "typeshed" \
    "d34ef50754de993d01630883dbcd1d27ba507143" &

# python 3.11, typing and 100% test coverage
clone_commit \
    "https://github.com/pypi/warehouse" \
    "warehouse" \
    "5a4d2cadec641b5d6a6847d0127940e0f532f184" &

# zulip, a django user
clone_commit \
    "https://github.com/zulip/zulip" \
    "zulip" \
    "ccddbba7a3074283ccaac3bde35fd32b19faf042" &

# home-assistant, home automation with 1ok files
clone_commit \
    "https://github.com/home-assistant/core" \
    "home-assistant" \
    "3601c531f400255d10b82529549e564fbe483a54" &

# poetry, a package manager that uses black preview style
clone_commit \
    "https://github.com/python-poetry/poetry" \
    "poetry" \
    "36fedb59b8e655252168055b536ead591068e1e4" &

# cpython itself
clone_commit \
    "https://github.com/python/cpython" \
    "cpython" \
    "28aea5d07d163105b42acd81c1651397ef95ea57" &

# wait for the concurrent clones to complete
wait

# Uncomment if you want to update the hashes
#for i in "$dir"/*/; do git -C "$i" switch main && git -C "$i" pull; done
#for i in "$dir"/*/; do echo "# $(basename "$i") $(git -C "$i" rev-parse HEAD)"; done

time cargo run --bin ruff_dev -- format-dev --stability-check \
  --error-file "$dir/errors.txt" \
  --log-file "$dir/log.txt" \
  --stats-file "$dir/stats.txt" \
  --files-with-errors 3 --multi-project "$dir/projects" \
|| (
  echo "Ecosystem check failed"
  cat "$dir/log.txt"
  exit 1
)

cat "$dir/stats.txt"
