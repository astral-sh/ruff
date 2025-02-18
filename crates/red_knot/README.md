# Red Knot

Red Knot is an extremely fast type checker.
Currently, it is a work-in-progress and not ready for user testing.

Red Knot is designed to prioritize good type inference, even in unannotated code,
and aims to avoid false positives.
It will have its own design choices and thus not be
a drop-in replacement for either Mypy or Pyright.

## Contributing

The crate structure is as follow:

- `red_knot`: Command line interface
- `red_knot_project`: Project discovering
- `red_knot_python_semantic`: Core type checking
- `red_knot_server`: Language server implementation
- `red_knot_test`: Type inference test framework
- `red_knot_vendored`: Public Red-Knot-specific Python APIs
- `ruff_db`: File-related infrastructure and rule registry

See their corresponding `README.md`, if any, for more information.

The list of open issues can be found [here][open-issues].

[open-issues]: https://github.com/astral-sh/ruff/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20label%3Ared-knot
