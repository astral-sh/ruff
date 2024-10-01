# Vendored types for the stdlib

This crate vendors [typeshed](https://github.com/python/typeshed)'s stubs for the standard library. The vendored stubs can be found in `crates/red_knot_vendored/vendor/typeshed`. The file `crates/red_knot_vendored/vendor/typeshed/source_commit.txt` tells you the typeshed commit that our vendored stdlib stubs currently correspond to.

The typeshed stubs are updated every two weeks via an automated PR using the `sync_typeshed.yaml` workflow in the `.github/workflows` directory. This workflow can also be triggered at any time via [workflow dispatch](https://docs.github.com/en/actions/using-workflows/manually-running-a-workflow#running-a-workflow).
