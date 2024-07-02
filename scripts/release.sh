#!/usr/bin/env bash
# Prepare for a release
#
# All additional options are passed to `rooster release`
set -eu

script_root="$(realpath "$(dirname "$0")")"
project_root="$(dirname "$script_root")"

echo "Updating metadata with rooster..."
cd "$project_root"
uv tool run --from rooster-blue --isolated -- rooster release "$@"

echo "Updating lockfile..."
cargo update -p ruff

echo "Generating contributors list..."
echo ""
echo ""
uv tool run --from rooster-blue --isolated -- rooster contributors --quiet
