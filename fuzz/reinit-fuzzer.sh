#!/bin/bash

# https://stackoverflow.com/a/246128/3549270
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$SCRIPT_DIR"

cd corpus/ruff_fix_validity
curl -L 'https://github.com/python/cpython/archive/refs/tags/v3.12.0b2.tar.gz' | tar xz
cp -r "../../../crates/ruff/resources/test" .
cd -
cargo fuzz cmin -s none ruff_fix_validity

echo "Done! You are ready to fuzz."
