#!/bin/bash

# https://stackoverflow.com/a/246128/3549270
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$SCRIPT_DIR"

if ! cargo fuzz --help >&/dev/null; then
  cargo install --git https://github.com/rust-fuzz/cargo-fuzz.git
fi

if [ ! -d corpus/ruff_parse_simple ]; then
  mkdir -p corpus/ruff_parse_simple
  read -p "Would you like to build a corpus from a python source code dataset? (this will take a long time!) [Y/n] " -n 1 -r
  echo
  if [[ $REPLY =~ ^[Yy]$ ]]; then
    cd corpus/ruff_parse_simple
    curl -L 'https://zenodo.org/record/3628784/files/python-corpus.tar.gz?download=1' | tar xz
    cd -
    cargo fuzz cmin -s none ruff_parse_simple
  fi
fi

echo "Done! You are ready to fuzz."
