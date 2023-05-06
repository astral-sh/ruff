#!/bin/bash
set -e

cd "$(dirname "$(dirname "$0")")"

python ast/asdl_rs.py --generic-file ast/src/generic.rs --located-file ast/src/located.rs --module-file ../RustPython/vm/src/stdlib/ast/gen.rs ast/Python.asdl
rustfmt ast/src/generic.rs ast/src/located.rs ../RustPython/vm/src/stdlib/ast/gen.rs
