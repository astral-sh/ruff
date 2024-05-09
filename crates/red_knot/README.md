# Red Knot

The Red Knot crate contains code working towards multifile analysis, type inference and, ultimately, type-checking. It's very much a work in progress for now.

## Vendored types for the stdlib

Red Knot vendors [typeshed](https://github.com/python/typeshed)'s stubs for the standard library. The vendored stubs can be found in `crates/red_knot/vendor/typeshed`. The file `crates/red_knot/vendor/typeshed/source_commit.txt` tells you the typeshed commit that our vendored stdlib stubs currently correspond to.

Updating the vendored stubs is currently done manually. On a Unix machine, follow the following steps (if you have a typeshed clone in a `typeshed` directory, and a Ruff clone in a `ruff` directory):

```shell
rm -rf ruff/crates/red_knot/vendor/typeshed
mkdir ruff/crates/red_knot/vendor/typeshed
cp typeshed/README.md ruff/crates/red_knot/vendor/typeshed
cp typeshed/LICENSE ruff/crates/red_knot/vendor/typeshed
cp -r typeshed/stdlib ruff/crates/red_knot/vendor/typeshed/stdlib
git -C typeshed rev-parse HEAD > ruff/crates/red_knot/vendor/typeshed/source_commit.txt
```
