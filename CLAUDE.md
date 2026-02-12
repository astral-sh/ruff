# Ruff Repository

This repository contains both Ruff (a Python linter and formatter) and ty (a Python type checker). The crates follow a naming convention: `ruff_*` for Ruff-specific code and `ty_*` for ty-specific code. ty reuses several Ruff crates, including the Python parser (`ruff_python_parser`) and AST definitions (`ruff_python_ast`).

## Running Tests

Run all tests (using `nextest` for faster execution):

```sh
cargo nextest run
```

For faster test execution, use the `fast-test` profile which enables optimizations while retaining debug info:

```sh
cargo nextest run --cargo-profile fast-test
```

Run tests for a specific crate:

```sh
cargo nextest run -p ty_python_semantic
```

Run a single mdtest file:

```sh
cargo nextest run -p ty_python_semantic --test mdtest -- mdtest::<path/to/mdtest_file.md>
```

To run a specific mdtest within a file, use a substring of the Markdown header text as `MDTEST_TEST_FILTER`. Only use this if it's necessary to isolate a single test case:

```sh
MDTEST_TEST_FILTER="<filter>" cargo nextest run -p ty_python_semantic --test mdtest -- mdtest::<path/to/mdtest_file.md>
```

Update snapshots after running tests:

```sh
cargo insta accept
```

## Running Clippy

```sh
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Running Debug Builds

Use debug builds (not `--release`) when developing, as release builds lack debug assertions and have slower compile times.

Run Ruff:

```sh
cargo run --bin ruff -- check path/to/file.py
```

Run ty:

```sh
cargo run --bin ty -- check path/to/file.py
```

## Reproducing ty ecosystem changes

If asked to reproduce changes in the ty ecosystem, use this script to clone the project to some
directory and install its dependencies into `.venv`:

```sh
uv run scripts/setup_primer_project.py <project-name> <some-temp-dir>
```

## Pull Requests

When working on ty, PR titles should start with `[ty]` and be tagged with the `ty` GitHub label.

## Development Guidelines

- All changes must be tested. If you're not testing your changes, you're not done.
- Get your tests to pass. If you didn't run the tests, your code does not work.
- Follow existing code style. Check neighboring files for patterns.
- Always run `uvx prek run -a` at the end of a task.
- Avoid writing significant amounts of new code. This is often a sign that we're missing an existing method or mechanism that could help solve the problem. Look for existing utilities first.
- Avoid falling back to patterns that require `panic!`, `unreachable!`, or `.unwrap()`. Instead, try to encode those constraints in the type system.
- Prefer let chains (`if let` combined with `&&`) over nested `if let` statements to reduce indentation and improve readability.
- If you *have* to suppress a Clippy lint, prefer to use `#[expect()]` over `[allow()]`, where possible.
- Use comments purposefully. Don't use comments to narrate code, but do use them to explain invariants and why something unusual was done a particular way.
