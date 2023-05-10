#!/bin/bash
set -e

cd "$(dirname "$(dirname "$0")")"

python ast/asdl_rs.py --generic-file ast/src/gen/generic.rs --located-file ast/src/gen/located.rs --module-file ../RustPython/vm/src/stdlib/ast/gen.rs ast/Python.asdl
rustfmt ast/src/gen/generic.rs ast/src/gen/located.rs ../RustPython/vm/src/stdlib/ast/gen.rs
