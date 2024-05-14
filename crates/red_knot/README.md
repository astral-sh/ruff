# Red Knot

The Red Knot crate contains code working towards multifile analysis, type inference and, ultimately, type-checking. It's very much a work in progress for now.

## Vendored types for the stdlib

Red Knot vendors [typeshed](https://github.com/python/typeshed)'s stubs for the standard library. The vendored stubs can be found in `crates/red_knot/vendor/typeshed`. The file `crates/red_knot/vendor/typeshed/source_commit.txt` tells you the typeshed commit that our vendored stdlib stubs currently correspond to.

The typeshed stubs are updated every two weeks via an automated PR using the `sync_typeshed.yaml` workflow in the `.github/workflows` directory. This workflow can also be triggered at any time via [workflow dispatch](https://docs.github.com/en/actions/using-workflows/manually-running-a-workflow#running-a-workflow).
