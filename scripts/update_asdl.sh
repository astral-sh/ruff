#!/bin/bash
set -e

cd "$(dirname "$(dirname "$0")")"

# rm ast/src/gen/*.rs
python ast/asdl_rs.py --ast-dir ast/src/gen/ --module-file ../RustPython/vm/src/stdlib/ast/gen.rs ast/Python.asdl
rustfmt ast/src/gen/*.rs ../RustPython/vm/src/stdlib/ast/gen.rs
