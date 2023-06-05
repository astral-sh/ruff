#!/bin/bash

# https://stackoverflow.com/a/246128/3549270
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$SCRIPT_DIR"

cd corpus/ruff_parse_simple
if [[ $REPLY =~ ^[Yy]$ ]]; then
  curl -L 'https://zenodo.org/record/3628784/files/python-corpus.tar.gz?download=1' | tar xz
fi
cp -r "../../../crates/ruff/resources/test" .
cd -
cargo fuzz cmin -s none ruff_fix_validity

echo "Done! You are ready to fuzz."
