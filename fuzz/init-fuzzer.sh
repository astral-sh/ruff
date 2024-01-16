#!/bin/bash

# https://stackoverflow.com/a/246128/3549270
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$SCRIPT_DIR"

if ! cargo fuzz --help >&/dev/null; then
  cargo install --git https://github.com/rust-fuzz/cargo-fuzz.git
fi

if [ ! -d corpus/ruff_fix_validity ]; then
  mkdir -p corpus/ruff_fix_validity
  read -p "Would you like to build a corpus from a python source code dataset? (this will take a long time!) [Y/n] " -n 1 -r
  echo
  cd corpus/ruff_fix_validity
  if [[ $REPLY =~ ^[Yy]$ ]]; then
    curl -L 'https://zenodo.org/record/3628784/files/python-corpus.tar.gz?download=1' | tar xz
  fi
  curl -L 'https://github.com/python/cpython/archive/refs/tags/v3.12.0b2.tar.gz' | tar xz
  cp -r "../../../crates/ruff_linter/resources/test" .
  cd -
  cargo fuzz cmin -s none ruff_fix_validity -- -timeout=5
fi

echo "Done! You are ready to fuzz."
