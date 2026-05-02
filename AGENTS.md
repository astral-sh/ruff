# Ruff Repository

This repository contains both Ruff (a Python linter and formatter) and ty (a Python type checker). The crates follow a naming convention: `ruff_*` for Ruff-specific code and `ty_*` for ty-specific code. ty reuses several Ruff crates, including the Python parser (`ruff_python_parser`) and AST definitions (`ruff_python_ast`).

## Running Tests

Run all tests (using `nextest` for faster execution, setting `CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_DEV_DEBUG="line-tables-only"` to enable optimizations while retaining some debug info, and setting `INSTA_FORCE_PASS=1 INSTA_UPDATE=always MDTEST_UPDATE_SNAPSHOTS=1` to ensure all snapshots are updated):

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run
```

Run tests for a specific crate:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic
```

Run a single mdtest file. The path to the mdtest file should be relative to the `crates/ty_python_semantic/resources/mdtest` folder:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::<path/to/mdtest_file.md>
```

To run a specific mdtest within a file, use a substring of the Markdown header text as `MDTEST_TEST_FILTER`. Only use this if it's necessary to isolate a single test case:

```sh
MDTEST_TEST_FILTER="<filter>" CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::<path/to/mdtest_file.md>
```

After running the tests, always review the contents of any snapshots that have been added or updated.

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

## Reproducing and minimizing ty ecosystem changes

If asked to reproduce changes in the ty ecosystem, use this script to clone the project to some
directory and install its dependencies into `.venv`:

```sh
uv run scripts/setup_primer_project.py <project-name> <some-temp-dir>
```

If asked to *minimize* a change in the ty ecosystem, you should start off with the above command to ensure that the change reproduces. You should then attempt to minimize the Python code required to demonstrate a behaviour difference between ty on your feature branch and ty on the main branch. Your minimization process should consist of systematically removing files from the cloned ecosystem project, and stripping content from existing files, until the behaviour difference between your branch and `main` no longer reproduces.

## Pull Requests

When working on ty, PR titles should start with `[ty]` and be tagged with the `ty` GitHub label.

## Development Guidelines

- All changes must be tested. If you're not testing your changes, you're not done.
- Look to see if your tests could go in an existing file before adding a new file for your tests.
- Get your tests to pass. If you didn't run the tests, your code does not work.
- Follow existing code style. Check neighboring files for patterns.
- Rust imports should always go at the top of the file, never locally in functions.
- Always run `uvx prek run -a` at the end of a task, after every rebase, after addressing any review comment, and before pushing any code.
- Avoid writing significant amounts of new code. This is often a sign that we're missing an existing method or mechanism that could help solve the problem. Look for existing utilities first.
- Try hard to avoid patterns that require `panic!`, `unreachable!`, or `.unwrap()`. Instead, try to encode those constraints in the type system. Don't be afraid to write code that's more verbose or requires largeish refactors if it enables you to avoid these unsafe calls.
- Prefer let chains (`if let` combined with `&&`) over nested `if let` statements to reduce indentation and improve readability. At the end of a task, always check your work to see if you missed opportunities to use `let` chains.
- If you *have* to suppress a Clippy lint, prefer to use `#[expect()]` over `[allow()]`, where possible. But if a lint is complaining about unused/dead code, it's usually best to just delete the unused code.
- Use comments purposefully. Don't use comments to narrate code, but do use them to explain invariants and why something unusual was done a particular way.
- When adding new ty checks, it's important to make error messages concise. Think about how an error message would look on a narrow terminal screen. Sometimes more detail can be provided in subdiagnostics or secondary annotations, but it's also important to make sure that the diagnostic is understandable if the user has passed `--output-format=concise`.
- **Salsa incrementality (ty):** Any method that accesses `.node()` must be `#[salsa::tracked]`, or it will break incrementality. Prefer higher-level semantic APIs over raw AST access.
- Run `cargo dev generate-all` after changing configuration options, CLI arguments, lint rules, or environment variable definitions, as these changes require regeneration of schemas, docs, and CLI references.
