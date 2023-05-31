#!/bin/bash
set -e

cd "$(dirname "$(dirname "$0")")"

# rm ast/src/gen/*.rs
python ast/asdl_rs.py --ast-dir ast/src/gen/ --parser-dir parser/src/gen/ --ast-pyo3-dir ast-pyo3/src/gen/ --module-file ../RustPython/vm/src/stdlib/ast/gen.rs ast/Python.asdl
rustfmt ast/src/gen/*.rs parser/src/gen/*.rs ast-pyo3/src/gen/*.rs ../RustPython/vm/src/stdlib/ast/gen.rs
