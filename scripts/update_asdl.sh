#!/bin/bash
set -e

cd "$(dirname "$(dirname "$0")")"

python ast/asdl_rs.py --ast-dir ast/src/gen/ --module-file ../RustPython/vm/src/stdlib/ast/gen.rs ast/Python.asdl
rustfmt ast/src/gen/*.rs ../RustPython/vm/src/stdlib/ast/gen.rs
