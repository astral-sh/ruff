# Contributing to ty

Welcome! We're happy to have you here. Thank you in advance for your contribution to ty.

> [!NOTE]
>
> This guide is for ty. If you're looking to contribute to Ruff, please see
> [the Ruff contributing guide](../../CONTRIBUTING.md).

## The Basics

We welcome contributions in the form of pull requests.

For small changes (e.g., bug fixes), feel free to submit a PR.

For larger changes (e.g. new diagnostics, new functionality, new configuration options), consider
creating an [**issue**](https://github.com/astral-sh/ty/issues) outlining your proposed change.
You can also join us on [Discord](https://discord.com/invite/astral-sh) to discuss your idea with the
community. We've labeled [beginner-friendly tasks](https://github.com/astral-sh/ty/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22)
in the issue tracker, along with [bugs](https://github.com/astral-sh/ty/issues?q=is%3Aissue+is%3Aopen+label%3Abug)
that are ready for contributions.

### Prerequisites

ty is written in Rust. You'll need to install the
[Rust toolchain](https://www.rust-lang.org/tools/install) for development.

You'll need [uv](https://docs.astral.sh/uv/getting-started/installation/) (or `pipx` and `pip`) to
run Python utility commands.

You can optionally install pre-commit hooks to automatically run the validation checks
when making a commit:

```shell
uv tool install pre-commit
pre-commit install
```

We recommend [nextest](https://nexte.st/) to run ty's test suite (via `cargo nextest run`),
though it's not strictly necessary:

```shell
cargo install cargo-nextest --locked
```

Throughout this guide, any usages of `cargo test` can be replaced with `cargo nextest run`,
if you choose to install `nextest`.

### Development

After cloning the repository, run ty locally from the repository root with:

```shell
cargo run --bin ty -- check --project /path/to/project/
```

Prior to opening a pull request, ensure that your code has been auto-formatted,
and that it passes both the lint and test validation checks:

```shell
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Rust linting
cargo test  # Rust testing
uvx pre-commit run --all-files --show-diff-on-failure  # Rust and Python formatting, Markdown and Python linting, etc.
```

These checks will run on GitHub Actions when you open your pull request, but running them locally
will save you time and expedite the merge process.

If you're using VS Code, you can also install the recommended [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) extension to get these checks while editing.

Include the text `[ty]` at the beginning of your pull request title, to distinguish ty pull requests
from Ruff ones.

Your pull request will be reviewed by a maintainer, which may involve a few rounds of iteration
prior to merging.

### Debugging ty

ty can optionally emit extensive tracing output, which can be very useful in understanding its
operation and debugging issues; see [`crates/ty/docs/tracing.md`](./docs/tracing.md) for details.

### Project Structure

The codebase is structured as a monorepo with a [flat crate structure](https://matklad.github.io/2021/08/22/large-rust-workspaces.html),
such that all crates are contained in a flat `crates` directory.

The vast majority of ty's code lives in the `ty_python_semantic` crate (located at
`crates/ty_python_semantic`). As a contributor, that's the crate that'll probably be most relevant
to you.

At the time of writing, the repository includes the following ty-specific crates (in addition to
crates shared with Ruff, such as `ruff_db`, `ruff_python_ast`, and `ruff_python_parser`):

- `ty_python_semantic`: The core type checker, which includes the type inference engine and
    semantic analysis.
- `ty_test`: The Markdown-based test framework for ty, "mdtest".
- `ty`: The command-line interface.
- `ty_ide`: IDE features (hover, go-to-definition, autocomplete) for the language server.
- `ty_project`: Discovery and representation of a Python project to be checked by ty.
- `ty_server`: The ty language server.
- `ty_vendored`: A vendored copy of [typeshed](https://github.com/python/typeshed), which holds type
    annotations for the Python standard library.
- `ty_wasm`: library crate for exposing ty as a WebAssembly module. Powers the
    [ty Playground](https://play.ty.dev/).

## Writing tests

Core type checking tests are written as Markdown code blocks.
They can be found in [`crates/ty_python_semantic/resources/mdtest`][resources-mdtest].
See [`crates/ty_test/README.md`][mdtest-readme] for more information
on the test framework itself.

Any ty pull request to improve ty's type inference or type checking logic should include mdtests
demonstrating the effect of the change.

We write mdtests in a "literate" style, with prose explaining the motivation of each test, and any
context necessary to understand the feature being demonstrated.

### Property tests

ty uses property-based testing to test the core type relations. These tests are located in
[`crates/ty_python_semantic/src/types/property_tests.rs`](../ty_python_semantic/src/types/property_tests.rs).

The property tests do not run in CI on every PR, just once daily. It is advisable to run them
locally after modifying core type relation methods (`is_subtype_of`, `is_equivalent_to`, etc.) to
ensure that the changes do not break any of the properties.

## Ecosystem CI (mypy-primer)

GitHub Actions will run your changes against a number of real-world projects from GitHub and
report on any linter or formatter differences. See [`crates/ty/docs/mypy_primer.md`](./docs/mypy_primer.md)
for instructions on running these checks locally.

## Coding guidelines

We use the [Salsa](https:://github.com/salsa-rs/salsa) library for incremental computation. Many
methods take a Salsa database (usually `db: &'db dyn Db`) as an argument. This should always be the
first argument (or second after `self`).

[mdtest-readme]: ../ty_test/README.md
[resources-mdtest]: ../ty_python_semantic/resources/mdtest
