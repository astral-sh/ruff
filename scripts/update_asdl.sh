#!/bin/bash
set -e

cd "$(dirname "$(dirname "$0")")"

# rm ast/src/gen/*.rs
python ast/asdl_rs.py --ast-dir ast/src/gen/ --parser-dir parser/src/gen/ ast/Python.asdl
rustfmt ast/src/gen/*.rs parser/src/gen/*.rs
