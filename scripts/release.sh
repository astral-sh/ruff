#!/usr/bin/env bash
# Prepare for a release
#
# All additional options are passed to `rooster release`
set -eu

export UV_PREVIEW=1

script_root="$(realpath "$(dirname "$0")")"
project_root="$(dirname "$script_root")"

echo "Updating metadata with rooster..."
cd "$project_root"
uv run --locked --python 3.12 --only-group release \
    rooster release "$@"

# Bump internal crate versions
uv run --script "$project_root/scripts/bump-workspace-crate-versions.py"

echo "Updating crate READMEs..."
uv run --script "$project_root/scripts/generate-crate-readmes.py"

echo "Updating lockfiles..."
cargo update -p ruff
uv lock --no-config

echo "Generating JSON schema..."
cargo dev generate-json-schema

echo "Checking crates.io publish setup..."
uv run --no-config --script "$project_root/scripts/setup-crates-io-publish.py" --quiet
