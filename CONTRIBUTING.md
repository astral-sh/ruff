# Contributing to ruff

Welcome! We're happy to have you here. Thank you in advance for your contribution to ruff.

## The basics

ruff welcomes contributions in the form of Pull Requests. For small changes (e.g., bug fixes), feel
free to submit a PR. For larger changes (e.g., new lint rules, new functionality, new configuration
options), consider submitting an [Issue](https://github.com/charliermarsh/ruff/issues) outlining
your proposed change.

### Prerequisites

ruff is written in Rust (1.63.0). You'll need to install the
[Rust toolchain](https://www.rust-lang.org/tools/install) for development.

You'll also need [Insta](https://insta.rs/docs/) to update snapshot tests:

```shell
cargo install cargo-insta
```

### Development

After cloning the repository, run ruff locally with:

```shell
cargo run resources/test/fixtures --no-cache
```

Prior to opening a pull request, ensure that your code has been auto-formatted, and that it passes
both the lint and test validation checks:

```shell
cargo fmt     # Auto-formatting...
cargo clippy  # Linting...
cargo test    # Testing...
```

These checks will run on GitHub Actions when you open your Pull Request, but running them locally
will save you time and expedite the merge process.

Your Pull Request will be reviewed by a maintainer, which may involve a few rounds of iteration
prior to merging.

### Example: Adding a new lint rule

There are three phases to adding a new lint rule:

1. Define the rule in `src/checks.rs`.
2. Define the _logic_ for triggering the rule in `src/check_ast.rs` (for AST-based checks)
   or `src/check_lines.rs` (for text-based checks).
3. Add a test fixture.

To define the rule, open up `src/checks.rs`. You'll need to define both a `CheckCode` and
`CheckKind`. As an example, you can grep for `E402` and `ModuleImportNotAtTopOfFile`, and follow the
pattern implemented therein.

To trigger the rule, you'll likely want to augment the logic in `src/check_ast.rs`, which defines
the Python AST visitor, responsible for iterating over the abstract syntax tree and collecting
lint-rule violations as it goes. Grep for the `Check::new` invocations to understand how other,
similar rules are implemented.

To add a test fixture, create a file under `resources/test/fixtures`, named to match the `CheckCode`
you defined earlier (e.g., `E402.py`). This file should contain a variety of violations and
non-violations designed to evaluate and demonstrate the behavior of your lint rule. Run ruff locally
with (e.g.) `cargo run resources/test/fixtures/E402.py`. Once you're satisfied with the output,
codify the behavior as a snapshot test by adding a new function to the `mod tests` section of
`src/linter.rs`, like so:

```rust
#[test]
fn e402() -> Result<()> {
    let mut checks = check_path(
        Path::new("./resources/test/fixtures/E402.py"),
        &settings::Settings::for_rule(CheckCode::E402),
        &fixer::Mode::Generate,
    )?;
    checks.sort_by_key(|check| check.location);
    insta::assert_yaml_snapshot!(checks);
    Ok(())
}
```

Then, run `cargo test`. Your test will fail, but you'll be prompted to follow-up with
`cargo insta review`. Accept the generated snapshot, then commit the snapshot file alongside the
rest of your changes.

### Example: Adding a new configuration option

ruff's user-facing settings live in two places: first, the command-line options defined with
[clap](https://docs.rs/clap/latest/clap/) via the `Cli` struct in `src/main.rs`; and second, the
`Config` struct defined `src/pyproject.rs`, which is responsible for extracting user-defined
settings from a `pyproject.toml` file.

Ultimately, these two sources of configuration are merged into the `Settings` struct defined
in `src/settings.rs`, which is then threaded through the codebase.

To add a new configuration option, you'll likely want to _both_ add a CLI option to `src/main.rs`
_and_ a `pyproject.toml` parameter to `src/pyproject.rs`. If you want to pattern-match against an
existing example, grep for `dummy_variable_rgx`, which defines a regular expression to match against
acceptable unused variables (e.g., `_`).

## Release process

As of now, ruff has an ad hoc release process: releases are cut with high frequency via GitHub
Actions, which automatically generates the appropriate wheels across architectures and publishes
them to [PyPI](https://pypi.org/project/ruff/).

ruff follows the [semver](https://semver.org/) versioning standard. However, as pre-1.0 software,
even patch releases may contain [non-backwards-compatible changes](https://semver.org/#spec-item-4).
