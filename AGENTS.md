# Ruff Repository

This repository contains both Ruff (a Python linter and formatter) and ty (a Python type checker). The crates follow a naming convention: `ruff_*` for Ruff-specific code and `ty_*` for ty-specific code. ty reuses several Ruff crates, including the Python parser (`ruff_python_parser`) and AST definitions (`ruff_python_ast`).

## Code reviews

When reviewing a branch or pull request, be deliberately nitpicky. Report not
only bugs and regressions, but also architectural and maintenance risks, weak
test coverage, unclear code, unnecessary complexity, and meaningful style or
consistency issues. Order findings by severity, cite files and lines, and
distinguish blockers from non-blocking improvements. Number each review point
for easy reference in subsequent review discussion.

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

### Fallback without nextest

If `cargo nextest` is not available, use `cargo test` with the same environment variables:

```sh
# Run all tests.
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo test

# Run tests for a specific crate.
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo test -p ty_python_semantic

# Run a single mdtest file.
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo test -p ty_python_semantic --test mdtest -- <path/to/mdtest_file.md>

# Run a specific mdtest within a file.
MDTEST_TEST_FILTER="<filter>" CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo test -p ty_python_semantic --test mdtest -- <path/to/mdtest_file.md>
```

### Snapshot updates

After running the tests, always review the contents of any snapshots that have been added or updated.

When running tests with `INSTA_FORCE_PASS=1`, check for `.pending-snap` files if any affected tests use inline snapshots.

Never edit snapshot files or inline snapshot bodies manually. Regenerate them by running the relevant tests with the snapshot-update environment variables documented above, then review the generated diff.

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

## Working on ty

The guidance in this section applies to edits to `ty*` crates, reviews of ty PRs, or other work when the ty type checker has been specifically mentioned by the user.

### Related skills

When the task matches a more specific ty workflow, also read and follow that skill from the repository root:

- Diagnostic changes, diagnostic message changes, or diagnostic reviews: `.agents/skills/adding-ty-diagnostics/SKILL.md`.
- Ecosystem report summaries: `.agents/skills/summarise-ecosystem-results/SKILL.md`.
- Reproducing, investigating, or minimizing ecosystem or primer differences: `.agents/skills/minimizing-ty-ecosystem-changes/SKILL.md`.

### Ad hoc reproductions

When running ty against a temporary Python reproduction file, create it outside the Ruff checkout (for example, under `/tmp`). A file inside the checkout discovers Ruff's root `pyproject.toml`, whose `requires-python = ">=3.7"` causes ty to infer Python 3.7 as the default Python version.

### PR conventions

When working on ty, PR titles should start with `[ty]`. Add the `ty` GitHub label if you have permission to do so;
if you don't, however, automation should add it anyway, so there's no need to worry about it. Similarly, add the `server`
label if your change only affects the LSP server and you have permission to add that label.

### The `db` parameter

For free functions and associated functions without a `self` parameter, `db` should be the first parameter. For methods with a `self` parameter, `db` should come immediately after `self`.

### Salsa tips

#### Tracked functions and methods

Adding `#[salsa::tracked]` to a function or method means that the Salsa framework will cache the function/method.
This can sometimes be done for performance reasons, and can also be done to ensure incremental computation in an
IDE context.

Methods that access `.node()` should usually be `#[salsa::tracked]`, or ty's incrementality will suffer:
we don't want to accidentally introduce a dependency on module `a`'s AST in a Salsa query that would be
called when type-checking module `b`. Prefer higher-level semantic APIs over raw AST access where possible,
but ask for guidance from the user if this would require significant refactoring.

#### Reduce memory usage where possible

For Salsa-cached values, avoid retaining excess collection capacity. Prefer boxed slices; otherwise shrink collections that may have spare capacity before returning them. In particular, inspect `HashMap` and `HashSet` values constructed via `extend`, `collect`, explicit reservation, or removal, since those operations can leave capacity that insert-only construction does not.

Salsa caching can occur due to a function/method having `#[salsa::tracked]` on it, or due to a struct with `#[salsa::interned]` being constructed.

## Generated Release Workflow

Parts of `.github/workflows/release.yml` are generated by cargo-dist from `dist-workspace.toml`. Before editing the release workflow, check whether the relevant section is generated. Prefer changing `dist-workspace.toml` or the referenced reusable workflow instead of editing generated YAML. After modifying cargo-dist configuration, regenerate the workflow with the cargo-dist version pinned in `dist-workspace.toml` and inspect the resulting diff to ensure the change will survive future regenerations.

## Development Guidelines

- All changes must be tested. If you're not testing your changes, you're not done.
- Look to see if your tests could go in an existing file before adding a new file for your tests.
- Get your tests to pass. If you didn't run the tests, your code does not work.
- Follow existing code style. Check neighboring files for patterns.
- Prefer narrow visibility by default because this workspace is generally its own consumer. However, do not add workarounds solely to avoid `pub`: make an item public when another workspace crate needs it and that produces the cleaner implementation.
- Rust imports should always go at the top of the file, never locally in functions.
- Run `uv run --only-group dev --locked prek` at the end of a task if you changed files in the repo. This includes changes such as rebases or addressing review comments. Use `uv run --only-group dev --locked prek run --files <path1> <path2>` and pass every file you changed. This keeps the hook run independent of staged state and avoids sweeping unrelated changes. Use `uv run --only-group dev --locked prek run --all-files` when a full-repository hook sweep is specifically needed.
- Avoid writing significant amounts of new code. This is often a sign that we're missing an existing method or mechanism that could help solve the problem. Look for existing utilities first.
- Try hard to avoid patterns that require `panic!`, `unreachable!`, or `.unwrap()`. Instead, try to encode those constraints in the type system. Don't be afraid to write code that's more verbose or requires largeish refactors if it enables you to avoid these unsafe calls.
- Prefer let chains (`if let` combined with `&&`) over nested `if let` statements to reduce indentation and improve readability. At the end of a task, always check your work to see if you missed opportunities to use `let` chains.
- If you *have* to suppress a Clippy lint, prefer to use `#[expect()]` over `[allow()]`, where possible. But if a lint is complaining about unused/dead code, it's usually best to just delete the unused code.
- Use comments purposefully. Don't use comments to narrate code, but do use them to explain invariants and why something unusual was done a particular way.
- Run `cargo dev generate-all` after changing configuration options, CLI arguments, lint rules, or environment variable definitions, as these changes require regeneration of schemas, docs, and CLI references.
- Don't prefix tests with `test_`.
- Don't separate struct definitions from their `impl` blocks unless the `impl` is deliberately placed in a separate file, as for large structs.
- Avoid running `uv run` for any scripts from the repository root unless you use `--no-project`, `--script` or similar. Using `uv run` from the Ruff repo root without these flags will build Ruff from source, which is very slow and usually unnecessary.
