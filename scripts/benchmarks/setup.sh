#!/usr/bin/env sh

###
# Setup the CPython repository to enable benchmarking.
###

git clone --branch 3.10 https://github.com/python/cpython.git crates/ruff_linter/resources/test/cpython --filter=blob:none
