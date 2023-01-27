# Contributing to Ruff

Welcome! We're happy to have you here. Thank you in advance for your contribution to Ruff.

## The basics

Ruff welcomes contributions in the form of Pull Requests. For small changes (e.g., bug fixes), feel
free to submit a PR. For larger changes (e.g., new lint rules, new functionality, new configuration
options), consider submitting an [Issue](https://github.com/charliermarsh/ruff/issues) outlining
your proposed change.

If you're looking for a place to start, we recommend implementing a new lint rule (see:
[_Adding a new lint rule_](#example-adding-a-new-lint-rule), which will allow you to learn from and
pattern-match against the examples in the existing codebase. Many lint rules are inspired by
existing Python plugins, which can be used as a reference implementation.

As a concrete example: consider taking on one of the rules from the [`tryceratops`](https://github.com/charliermarsh/ruff/issues/2056)
plugin, and looking to the originating [Python source](https://github.com/guilatrova/tryceratops)
for guidance. [`flake8-simplify`](https://github.com/charliermarsh/ruff/issues/998) has a few rules
left too.

### Prerequisites

Ruff is written in Rust. You'll need to install the
[Rust toolchain](https://www.rust-lang.org/tools/install) for development.

You'll also need [Insta](https://insta.rs/docs/) to update snapshot tests:

```shell
cargo install cargo-insta
```

### Development

After cloning the repository, run Ruff locally with:

```shell
cargo run resources/test/fixtures --no-cache
```

Prior to opening a pull request, ensure that your code has been auto-formatted,
and that it passes both the lint and test validation checks:

```shell
cargo fmt --all     # Auto-formatting...
cargo clippy --fix --workspace --all-targets --all-features  # Linting...
cargo test --all    # Testing...
```

These checks will run on GitHub Actions when you open your Pull Request, but running them locally
will save you time and expedite the merge process.

If you have `pre-commit` [installed](https://pre-commit.com/#installation) then you can use it to 
assist with formatting and linting. The following command will run the `pre-commit` hooks:

```shell
pre-commit run --all-files
```

Your Pull Request will be reviewed by a maintainer, which may involve a few rounds of iteration
prior to merging.

### Example: Adding a new lint rule

At a high level, the steps involved in adding a new lint rule are as follows:

1. Create a file for your rule (e.g., `src/rules/flake8_bugbear/rules/abstract_base_class.rs`).
2. In that file, define a violation struct. You can grep for `define_violation!` to see examples.
3. Map the violation struct to a rule code in `src/registry.rs` (e.g., `E402`).
4. Define the logic for triggering the violation in `src/checkers/ast.rs` (for AST-based checks),
   `src/checkers/tokens.rs` (for token-based checks), `src/checkers/lines.rs` (for text-based
   checks), or `src/checkers/filesystem.rs` (for filesystem-based checks).
5. Add a test fixture.
6. Update the generated files (documentation and generated code).

To define the violation, start by creating a dedicated file for your rule under the appropriate
rule linter (e.g., `src/rules/flake8_bugbear/rules/abstract_base_class.rs`). That file should
contain a struct defined via `define_violation!`, along with a function that creates the violation
based on any required inputs. (Many of the existing examples live in `src/violations.rs`, but we're
looking to place new rules in their own files.)

To trigger the violation, you'll likely want to augment the logic in `src/checkers/ast.rs`, which
defines the Python AST visitor, responsible for iterating over the abstract syntax tree and
collecting diagnostics as it goes.

If you need to inspect the AST, you can run `cargo dev print-ast` with a Python file. Grep
for the `Check::new` invocations to understand how other, similar rules are implemented.

To add a test fixture, create a file under `resources/test/fixtures/[linter]`, named to match
the code you defined earlier (e.g., `resources/test/fixtures/pycodestyle/E402.py`). This file should
contain a variety of violations and non-violations designed to evaluate and demonstrate the behavior
of your lint rule.

Run `cargo dev generate-all` to generate the code for your new fixture. Then run Ruff
locally with (e.g.) `cargo run resources/test/fixtures/pycodestyle/E402.py --no-cache --select E402`.

Once you're satisfied with the output, codify the behavior as a snapshot test by adding a new
`test_case` macro in the relevant `src/[linter]/mod.rs` file. Then, run `cargo test --all`.
Your test will fail, but you'll be prompted to follow-up with `cargo insta review`. Accept the
generated snapshot, then commit the snapshot file alongside the rest of your changes.

Finally, regenerate the documentation and generated code with `cargo dev generate-all`.

### Example: Adding a new configuration option

Ruff's user-facing settings live in a few different places.

First, the command-line options are defined via the `Cli` struct in `src/cli.rs`.

Second, the `pyproject.toml` options are defined in `src/settings/options.rs` (via the `Options`
struct), `src/settings/configuration.rs` (via the `Configuration` struct), and `src/settings/mod.rs`
(via the `Settings` struct). These represent, respectively: the schema used to parse the
`pyproject.toml` file; an internal, intermediate representation; and the final, internal
representation used to power Ruff.

To add a new configuration option, you'll likely want to modify these latter few files (along with
`cli.rs`, if appropriate). If you want to pattern-match against an existing example, grep for
`dummy_variable_rgx`, which defines a regular expression to match against acceptable unused
variables (e.g., `_`).

Note that plugin-specific configuration options are defined in their own modules (e.g.,
`src/flake8_unused_arguments/settings.rs`).

You may also want to add the new configuration option to the `flake8-to-ruff` tool, which is
responsible for converting `flake8` configuration files to Ruff's TOML format. This logic
lives in `flake8_to_ruff/src/converter.rs`.

Finally, regenerate the documentation and generated code with `cargo dev generate-all`.

## Release process

As of now, Ruff has an ad hoc release process: releases are cut with high frequency via GitHub
Actions, which automatically generates the appropriate wheels across architectures and publishes
them to [PyPI](https://pypi.org/project/ruff/).

Ruff follows the [semver](https://semver.org/) versioning standard. However, as pre-1.0 software,
even patch releases may contain [non-backwards-compatible changes](https://semver.org/#spec-item-4).
