#!/bin/bash

# https://stackoverflow.com/a/246128/3549270
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$SCRIPT_DIR"

if ! cargo fuzz --help >&/dev/null; then
  cargo install --git https://github.com/rust-fuzz/cargo-fuzz.git
fi

if [ ! -d corpus/ruff_fix_validity ]; then
  mkdir -p corpus/ruff_fix_validity

  (
    cd corpus/ruff_fix_validity

    read -p "Would you like to build a corpus from a python source code dataset? (this will take a long time!) [Y/n] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
      curl -L 'https://zenodo.org/record/3628784/files/python-corpus.tar.gz?download=1' | tar xz
    fi

    # Build a smaller corpus in addition to the (optional) larger corpus
    curl -L 'https://github.com/python/cpython/archive/refs/tags/v3.13.0.tar.gz' | tar xz
    cp -r "../../../crates/red_knot_project/resources/test/corpus" "red_knot_project"
    cp -r "../../../crates/ruff_linter/resources/test/fixtures" "ruff_linter"
    cp -r "../../../crates/ruff_python_formatter/resources/test/fixtures" "ruff_python_formatter"
    cp -r "../../../crates/ruff_python_parser/resources" "ruff_python_parser"

    # Delete all non-Python files
    find . -type f -not -name "*.py" -delete
  )

  if [[ "$OSTYPE" == "darwin"* ]]; then
    cargo +nightly fuzz cmin ruff_fix_validity -- -timeout=5
  else
    cargo fuzz cmin -s none ruff_fix_validity -- -timeout=5
  fi
fi

echo "Done! You are ready to fuzz."
