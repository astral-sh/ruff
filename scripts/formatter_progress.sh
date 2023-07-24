#!/usr/bin/env bash

# The pinned revisions are the latest of this writing, update freely

set -ex

target=$(git rev-parse --show-toplevel)/target
dir="$target/progress_projects"
mkdir -p "$dir"

# small util library
if [ ! -d "$dir/build" ]; then
  git clone --filter=tree:0 https://github.com/pypa/build "$dir/build"
  git -C "$dir/build" checkout d90f9ac6503a40ddbfaef94b7a7040f87178a4b3
fi
# web framework that implements a lot of magic
if [ ! -d "$dir/django" ]; then
  git clone --filter=tree:0 https://github.com/django/django "$dir/django"
  git -C "$dir/django" checkout 95e4d6b81312fdd9f8ebf3385be1c1331168b5cf
fi
# an ML project
if [ ! -d "$dir/transformers" ]; then
  git clone --filter=tree:0 https://github.com/huggingface/transformers "$dir/transformers"
  git -C "$dir/transformers" checkout c9a82be592ca305180a7ab6a36e884bca1d426b8
fi
# type annotations
if [ ! -d "$dir/typeshed" ]; then
  git clone --filter=tree:0 https://github.com/python/typeshed "$dir/typeshed"
  git -C "$dir/typeshed" checkout 7d33060e6ae3ebe54462a891f0c566c97371915b
fi
# python 3.11, typing and 100% test coverage
if [ ! -d "$dir/warehouse" ]; then
  git clone --filter=tree:0 https://github.com/pypi/warehouse "$dir/warehouse"
  git -C "$dir/warehouse" checkout fe6455c0a946e81f61d72edc1049f536d8bba903
fi
# django project
if [ ! -d "$dir/zulip" ]; then
  git clone --filter=tree:0 https://github.com/zulip/zulip "$dir/zulip"
  git -C "$dir/zulip" checkout 6cb080c4479546a7f5cb017fcddea56605910b48
fi

# Uncomment if you want to update the hashes
# for i in "$dir"/*/; do git -C "$i" switch main && git -C "$i" pull && echo "# $(basename "$i") $(git -C "$i" rev-parse HEAD)"; done

time cargo run --bin ruff_dev -- format-dev --stability-check --error-file "$target/progress_projects_errors.txt" \
  --multi-project "$dir" >"$target/progress_projects_report.txt"
grep "similarity index" "$target/progress_projects_report.txt" | sort
