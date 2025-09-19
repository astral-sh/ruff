#!/usr/bin/env bash

# This script uses the https://github.com/astral-sh/docstring-adder tool to codemod docstrings into our vendored typeshed stubs.
#
# We run the tool with the full matrix of Python versions supported by typeshed,
# so that we codemod in docstrings that only exist on certain versions.
#
# The codemod will only add docstrings to functions/classes that do not
# already have docstrings. We run with Python 3.14 before running with
# any other Python version so that we get the Python 3.14 version of the
# docstring for a definition that exists on all Python versions: if we
# ran with Python 3.9 first, then the later runs with Python 3.10+ would
# not modify the docstring that had already been added using the old version of Python.
#
# Note that the codemod can only add docstrings if they exist on the Python platform
# the codemod is run with. If you need to add docstrings for a Windows-specific API,
# you'll need to run the codemod on a Windows machine.

set -eu

docstring_adder="git+https://github.com/astral-sh/docstring-adder.git@93e8fdf5f65410c2aa88bc8523e3fc2a598e3917"
stdlib_path="./crates/ty_vendored/vendor/typeshed/stdlib"

for python_version in 3.14 3.13 3.12 3.11 3.10 3.9
do
  PYTHONUTF8=1 uvx --python="$python_version" --force-reinstall --from="${docstring_adder}" add-docstrings --stdlib-path="${stdlib_path}"
done
