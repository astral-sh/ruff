#!/bin/bash
set -eu

# https://stackoverflow.com/a/246128/3549270
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "$SCRIPT_DIR"

if ! cargo fuzz --help >&/dev/null; then
  echo "Installing cargo-fuzz..."
  cargo install --git https://github.com/rust-fuzz/cargo-fuzz.git
fi

if [ ! -d corpus/common ]; then
  mkdir -p corpus/common

  echo "Creating symlinks for fuzz targets to the common corpus directory..."
  for target in fuzz_targets/*; do
    corpus_dir="$(basename "$target" .rs)"
    ln -vs "./common" "corpus/$corpus_dir"
  done

  (
    cd corpus/common

    read -p "Would you like to build a corpus from a python source code dataset? (this will take a long time!) [Y/n] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
      echo "Downloading the Python source code dataset..."
      curl -L 'https://zenodo.org/record/3628784/files/python-corpus.tar.gz?download=1' | tar xz
    fi

    # Build a smaller corpus in addition to the (optional) larger corpus
    echo "Building a smaller corpus dataset..."
    curl -L 'https://github.com/python/cpython/archive/refs/tags/v3.13.0.tar.gz' | tar xz
    cp -r "../../../crates/ty_project/resources/test/corpus" "ty_project"
    cp -r "../../../crates/ruff_linter/resources/test/fixtures" "ruff_linter"
    cp -r "../../../crates/ruff_python_formatter/resources/test/fixtures" "ruff_python_formatter"
    cp -r "../../../crates/ruff_python_parser/resources" "ruff_python_parser"

    # Delete all non-Python files
    find . -type f -not -name "*.py" -delete
  )

  echo "Minifying the corpus dataset..."
  if [[ "$OSTYPE" == "darwin"* ]]; then
    cargo +nightly fuzz cmin ruff_fix_validity corpus/common -- -timeout=5
  else
    cargo fuzz cmin -s none ruff_fix_validity corpus/common -- -timeout=5
  fi
fi

echo "Done! You are ready to fuzz"
