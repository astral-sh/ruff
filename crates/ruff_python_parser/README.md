# Ruff Python Parser

Ruff's Python parser is a hand-written [recursive descent parser] which can parse
Python source code into an [Abstract Syntax Tree (AST)](https://crates.io/crates/ruff_python_ast). It also utilizes the [Pratt
parsing](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html)
technique to parse expressions with different [precedence](https://docs.python.org/3/reference/expressions.html#operator-precedence).

Try out the parser in the [playground](https://play.ruff.rs/?secondary=AST).

## Contributing

Refer to the [contributing guidelines](./CONTRIBUTING.md) to get started and GitHub issues with the
[parser label](https://github.com/astral-sh/ruff/issues?q=is:open+is:issue+label:parser) for issues that need help.

## Versioning

<!-- BEGIN GENERATED CRATE VERSIONING -->

This crate is an internal component of [Ruff](https://crates.io/crates/ruff). The Rust API exposed
here is unstable and will have frequent breaking changes.

This version (0.0.5) is a component of [Ruff 0.15.22](https://crates.io/crates/ruff/0.15.22). The
source can be found [here](https://github.com/astral-sh/ruff/blob/0.15.22/crates/ruff_python_parser).

See Ruff's [crate versioning policy](https://docs.astral.sh/ruff/versioning/#crate-versioning) for
details on versioning.

<!-- END GENERATED CRATE VERSIONING -->

[recursive descent parser]: https://en.wikipedia.org/wiki/Recursive_descent_parser
