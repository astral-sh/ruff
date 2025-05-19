# Contributing to Ruff

Welcome! We're happy to have you here. Thank you in advance for your contribution to Ruff.

> [!NOTE]
>
> This guide is for Ruff. If you're looking to contribute to ty, please see [the ty contributing
> guide](https://github.com/astral-sh/ruff/blob/main/crates/ty/CONTRIBUTING.md).

## The Basics

Ruff welcomes contributions in the form of pull requests.

For small changes (e.g., bug fixes), feel free to submit a PR.

For larger changes (e.g., new lint rules, new functionality, new configuration options), consider
creating an [**issue**](https://github.com/astral-sh/ruff/issues) outlining your proposed change.
You can also join us on [Discord](https://discord.com/invite/astral-sh) to discuss your idea with the
community. We've labeled [beginner-friendly tasks](https://github.com/astral-sh/ruff/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
in the issue tracker, along with [bugs](https://github.com/astral-sh/ruff/issues?q=is%3Aissue+is%3Aopen+label%3Abug)
and [improvements](https://github.com/astral-sh/ruff/issues?q=is%3Aissue+is%3Aopen+label%3Aaccepted)
that are ready for contributions.

If you have suggestions on how we might improve the contributing documentation, [let us know](https://github.com/astral-sh/ruff/discussions/5693)!

### Prerequisites

Ruff is written in Rust. You'll need to install the
[Rust toolchain](https://www.rust-lang.org/tools/install) for development.

You'll also need [Insta](https://insta.rs/docs/) to update snapshot tests:

```shell
cargo install cargo-insta
```

You'll need [uv](https://docs.astral.sh/uv/getting-started/installation/) (or `pipx` and `pip`) to
run Python utility commands.

You can optionally install pre-commit hooks to automatically run the validation checks
when making a commit:

```shell
uv tool install pre-commit
pre-commit install
```

We recommend [nextest](https://nexte.st/) to run Ruff's test suite (via `cargo nextest run`),
though it's not strictly necessary:

```shell
cargo install cargo-nextest --locked
```

Throughout this guide, any usages of `cargo test` can be replaced with `cargo nextest run`,
if you choose to install `nextest`.

### Development

After cloning the repository, run Ruff locally from the repository root with:

```shell
cargo run -p ruff -- check /path/to/file.py --no-cache
```

Prior to opening a pull request, ensure that your code has been auto-formatted,
and that it passes both the lint and test validation checks:

```shell
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Rust linting
RUFF_UPDATE_SCHEMA=1 cargo test  # Rust testing and updating ruff.schema.json
uvx pre-commit run --all-files --show-diff-on-failure  # Rust and Python formatting, Markdown and Python linting, etc.
```

These checks will run on GitHub Actions when you open your pull request, but running them locally
will save you time and expedite the merge process.

If you're using VS Code, you can also install the recommended [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) extension to get these checks while editing.

Note that many code changes also require updating the snapshot tests, which is done interactively
after running `cargo test` like so:

```shell
cargo insta review
```

If your pull request relates to a specific lint rule, include the category and rule code in the
title, as in the following examples:

- \[`flake8-bugbear`\] Avoid false positive for usage after `continue` (`B031`)
- \[`flake8-simplify`\] Detect implicit `else` cases in `needless-bool` (`SIM103`)
- \[`pycodestyle`\] Implement `redundant-backslash` (`E502`)

Your pull request will be reviewed by a maintainer, which may involve a few rounds of iteration
prior to merging.

### Project Structure

Ruff is structured as a monorepo with a [flat crate structure](https://matklad.github.io/2021/08/22/large-rust-workspaces.html),
such that all crates are contained in a flat `crates` directory.

The vast majority of the code, including all lint rules, lives in the `ruff_linter` crate (located
at `crates/ruff_linter`). As a contributor, that's the crate that'll be most relevant to you.

At the time of writing, the repository includes the following crates:

- `crates/ruff_linter`: library crate containing all lint rules and the core logic for running them.
    If you're working on a rule, this is the crate for you.
- `crates/ruff_benchmark`: binary crate for running micro-benchmarks.
- `crates/ruff_cache`: library crate for caching lint results.
- `crates/ruff`: binary crate containing Ruff's command-line interface.
- `crates/ruff_dev`: binary crate containing utilities used in the development of Ruff itself (e.g.,
    `cargo dev generate-all`), see the [`cargo dev`](#cargo-dev) section below.
- `crates/ruff_diagnostics`: library crate for the rule-independent abstractions in the lint
    diagnostics APIs.
- `crates/ruff_formatter`: library crate for language agnostic code formatting logic based on an
    intermediate representation. The backend for `ruff_python_formatter`.
- `crates/ruff_index`: library crate inspired by `rustc_index`.
- `crates/ruff_macros`: proc macro crate containing macros used by Ruff.
- `crates/ruff_notebook`: library crate for parsing and manipulating Jupyter notebooks.
- `crates/ruff_python_ast`: library crate containing Python-specific AST types and utilities.
- `crates/ruff_python_codegen`: library crate containing utilities for generating Python source code.
- `crates/ruff_python_formatter`: library crate implementing the Python formatter. Emits an
    intermediate representation for each node, which `ruff_formatter` prints based on the configured
    line length.
- `crates/ruff_python_semantic`: library crate containing Python-specific semantic analysis logic,
    including Ruff's semantic model. Used to resolve queries like "What import does this variable
    refer to?"
- `crates/ruff_python_stdlib`: library crate containing Python-specific standard library data, e.g.
    the names of all built-in exceptions and which standard library types are immutable.
- `crates/ruff_python_trivia`: library crate containing Python-specific trivia utilities (e.g.,
    for analyzing indentation, newlines, etc.).
- `crates/ruff_python_parser`: library crate containing the Python parser.
- `crates/ruff_wasm`: library crate for exposing Ruff as a WebAssembly module. Powers the
    [Ruff Playground](https://play.ruff.rs/).

### Example: Adding a new lint rule

At a high level, the steps involved in adding a new lint rule are as follows:

1. Determine a name for the new rule as per our [rule naming convention](#rule-naming-convention)
    (e.g., `AssertFalse`, as in, "allow `assert False`").

1. Create a file for your rule (e.g., `crates/ruff_linter/src/rules/flake8_bugbear/rules/assert_false.rs`).

1. In that file, define a violation struct (e.g., `pub struct AssertFalse`). You can grep for
    `#[derive(ViolationMetadata)]` to see examples.

1. In that file, define a function that adds the violation to the diagnostic list as appropriate
    (e.g., `pub(crate) fn assert_false`) based on whatever inputs are required for the rule (e.g.,
    an `ast::StmtAssert` node).

1. Define the logic for invoking the diagnostic in `crates/ruff_linter/src/checkers/ast/analyze` (for
    AST-based rules), `crates/ruff_linter/src/checkers/tokens.rs` (for token-based rules),
    `crates/ruff_linter/src/checkers/physical_lines.rs` (for text-based rules),
    `crates/ruff_linter/src/checkers/filesystem.rs` (for filesystem-based rules), etc. For AST-based rules,
    you'll likely want to modify `analyze/statement.rs` (if your rule is based on analyzing
    statements, like imports) or `analyze/expression.rs` (if your rule is based on analyzing
    expressions, like function calls).

1. Map the violation struct to a rule code in `crates/ruff_linter/src/codes.rs` (e.g., `B011`). New rules
    should be added in `RuleGroup::Preview`.

1. Add proper [testing](#rule-testing-fixtures-and-snapshots) for your rule.

1. Update the generated files (documentation and generated code).

To trigger the violation, you'll likely want to augment the logic in `crates/ruff_linter/src/checkers/ast.rs`
to call your new function at the appropriate time and with the appropriate inputs. The `Checker`
defined therein is a Python AST visitor, which iterates over the AST, building up a semantic model,
and calling out to lint rule analyzer functions as it goes.

If you need to inspect the AST, you can run `cargo dev print-ast` with a Python file. Grep
for the `Diagnostic::new` invocations to understand how other, similar rules are implemented.

Once you're satisfied with your code, add tests for your rule
(see: [rule testing](#rule-testing-fixtures-and-snapshots)), and regenerate the documentation and
associated assets (like our JSON Schema) with `cargo dev generate-all`.

Finally, submit a pull request, and include the category, rule name, and rule code in the title, as
in:

> \[`pycodestyle`\] Implement `redundant-backslash` (`E502`)

#### Rule naming convention

Like Clippy, Ruff's rule names should make grammatical and logical sense when read as "allow
${rule}" or "allow ${rule} items", as in the context of suppression comments.

For example, `AssertFalse` fits this convention: it flags `assert False` statements, and so a
suppression comment would be framed as "allow `assert False`".

As such, rule names should...

- Highlight the pattern that is being linted against, rather than the preferred alternative.
    For example, `AssertFalse` guards against `assert False` statements.

- _Not_ contain instructions on how to fix the violation, which instead belong in the rule
    documentation and the `fix_title`.

- _Not_ contain a redundant prefix, like `Disallow` or `Banned`, which are already implied by the
    convention.

When re-implementing rules from other linters, we prioritize adhering to this convention over
preserving the original rule name.

#### Rule testing: fixtures and snapshots

To test rules, Ruff uses snapshots of Ruff's output for a given file (fixture). Generally, there
will be one file per rule (e.g., `E402.py`), and each file will contain all necessary examples of
both violations and non-violations. `cargo insta review` will generate a snapshot file containing
Ruff's output for each fixture, which you can then commit alongside your changes.

Once you've completed the code for the rule itself, you can define tests with the following steps:

1. Add a Python file to `crates/ruff_linter/resources/test/fixtures/[linter]` that contains the code you
    want to test. The file name should match the rule name (e.g., `E402.py`), and it should include
    examples of both violations and non-violations.

1. Run Ruff locally against your file and verify the output is as expected. Once you're satisfied
    with the output (you see the violations you expect, and no others), proceed to the next step.
    For example, if you're adding a new rule named `E402`, you would run:

    ```shell
    cargo run -p ruff -- check crates/ruff_linter/resources/test/fixtures/pycodestyle/E402.py --no-cache --preview --select E402
    ```

    **Note:** Only a subset of rules are enabled by default. When testing a new rule, ensure that
    you activate it by adding `--select ${rule_code}` to the command.

1. Add the test to the relevant `crates/ruff_linter/src/rules/[linter]/mod.rs` file. If you're contributing
    a rule to a pre-existing set, you should be able to find a similar example to pattern-match
    against. If you're adding a new linter, you'll need to create a new `mod.rs` file (see,
    e.g., `crates/ruff_linter/src/rules/flake8_bugbear/mod.rs`)

1. Run `cargo test`. Your test will fail, but you'll be prompted to follow-up
    with `cargo insta review`. Run `cargo insta review`, review and accept the generated snapshot,
    then commit the snapshot file alongside the rest of your changes.

1. Run `cargo test` again to ensure that your test passes.

### Example: Adding a new configuration option

Ruff's user-facing settings live in a few different places.

First, the command-line options are defined via the `Args` struct in `crates/ruff/src/args.rs`.

Second, the `pyproject.toml` options are defined in `crates/ruff_workspace/src/options.rs` (via the
`Options` struct), `crates/ruff_workspace/src/configuration.rs` (via the `Configuration` struct),
and `crates/ruff_workspace/src/settings.rs` (via the `Settings` struct), which then includes
the `LinterSettings` struct as a field.

These represent, respectively: the schema used to parse the `pyproject.toml` file; an internal,
intermediate representation; and the final, internal representation used to power Ruff.

To add a new configuration option, you'll likely want to modify these latter few files (along with
`args.rs`, if appropriate). If you want to pattern-match against an existing example, grep for
`dummy_variable_rgx`, which defines a regular expression to match against acceptable unused
variables (e.g., `_`).

Note that plugin-specific configuration options are defined in their own modules (e.g.,
`Settings` in `crates/ruff_linter/src/flake8_unused_arguments/settings.rs` coupled with
`Flake8UnusedArgumentsOptions` in `crates/ruff_workspace/src/options.rs`).

Finally, regenerate the documentation and generated code with `cargo dev generate-all`.

## MkDocs

To preview any changes to the documentation locally:

1. Install the [Rust toolchain](https://www.rust-lang.org/tools/install).

1. Generate the MkDocs site with:

    ```shell
    uv run --no-project --isolated --with-requirements docs/requirements.txt scripts/generate_mkdocs.py
    ```

1. Run the development server with:

    ```shell
    # For contributors.
    uvx --with-requirements docs/requirements.txt -- mkdocs serve -f mkdocs.public.yml

    # For members of the Astral org, which has access to MkDocs Insiders via sponsorship.
    uvx --with-requirements docs/requirements-insiders.txt -- mkdocs serve -f mkdocs.insiders.yml
    ```

The documentation should then be available locally at
[http://127.0.0.1:8000/ruff/](http://127.0.0.1:8000/ruff/).

## Release Process

As of now, Ruff has an ad hoc release process: releases are cut with high frequency via GitHub
Actions, which automatically generates the appropriate wheels across architectures and publishes
them to [PyPI](https://pypi.org/project/ruff/).

Ruff follows the [semver](https://semver.org/) versioning standard. However, as pre-1.0 software,
even patch releases may contain [non-backwards-compatible changes](https://semver.org/#spec-item-4).

### Creating a new release

1. Install `uv`: `curl -LsSf https://astral.sh/uv/install.sh | sh`

1. Run `./scripts/release.sh`; this command will:

    - Generate a temporary virtual environment with `rooster`
    - Generate a changelog entry in `CHANGELOG.md`
    - Update versions in `pyproject.toml` and `Cargo.toml`
    - Update references to versions in the `README.md` and documentation
    - Display contributors for the release

1. The changelog should then be editorialized for consistency

    - Often labels will be missing from pull requests they will need to be manually organized into the proper section
    - Changes should be edited to be user-facing descriptions, avoiding internal details

1. Highlight any breaking changes in `BREAKING_CHANGES.md`

1. Run `cargo check`. This should update the lock file with new versions.

1. Create a pull request with the changelog and version updates

1. Merge the PR

1. Run the [release workflow](https://github.com/astral-sh/ruff/actions/workflows/release.yml) with:

    - The new version number (without starting `v`)

1. The release workflow will do the following:

    1. Build all the assets. If this fails (even though we tested in step 4), we haven't tagged or
        uploaded anything, you can restart after pushing a fix. If you just need to rerun the build,
        make sure you're [re-running all the failed
        jobs](https://docs.github.com/en/actions/managing-workflow-runs/re-running-workflows-and-jobs#re-running-failed-jobs-in-a-workflow) and not just a single failed job.
    1. Upload to PyPI.
    1. Create and push the Git tag (as extracted from `pyproject.toml`). We create the Git tag only
        after building the wheels and uploading to PyPI, since we can't delete or modify the tag ([#4468](https://github.com/astral-sh/ruff/issues/4468)).
    1. Attach artifacts to draft GitHub release
    1. Trigger downstream repositories. This can fail non-catastrophically, as we can run any
        downstream jobs manually if needed.

1. Verify the GitHub release:

    1. The Changelog should match the content of `CHANGELOG.md`
    1. Append the contributors from the `scripts/release.sh` script

1. If needed, [update the schemastore](https://github.com/astral-sh/ruff/blob/main/scripts/update_schemastore.py).

    1. One can determine if an update is needed when
        `git diff old-version-tag new-version-tag -- ruff.schema.json` returns a non-empty diff.
    1. Once run successfully, you should follow the link in the output to create a PR.

1. If needed, update the [`ruff-lsp`](https://github.com/astral-sh/ruff-lsp) and
    [`ruff-vscode`](https://github.com/astral-sh/ruff-vscode) repositories and follow
    the release instructions in those repositories. `ruff-lsp` should always be updated
    before `ruff-vscode`.

    This step is generally not required for a patch release, but should always be done
    for a minor release.

## Ecosystem CI

GitHub Actions will run your changes against a number of real-world projects from GitHub and
report on any linter or formatter differences. You can also run those checks locally via:

```shell
uvx --from ./python/ruff-ecosystem ruff-ecosystem check ruff "./target/debug/ruff"
uvx --from ./python/ruff-ecosystem ruff-ecosystem format ruff "./target/debug/ruff"
```

See the [ruff-ecosystem package](https://github.com/astral-sh/ruff/tree/main/python/ruff-ecosystem) for more details.

## Upgrading Rust

1. Change the `channel` in `./rust-toolchain.toml` to the new Rust version (`<latest>`)
1. Change the `rust-version` in the `./Cargo.toml` to `<latest> - 2` (e.g. 1.84 if the latest is 1.86)
1. Run `cargo clippy --fix --allow-dirty --allow-staged` to fix new clippy warnings
1. Create and merge the PR
1. Bump the Rust version in Ruff's conda forge recipe. See [this PR](https://github.com/conda-forge/ruff-feedstock/pull/266) for an example.
1. Enjoy the new Rust version!

## Benchmarking and Profiling

We have several ways of benchmarking and profiling Ruff:

- Our main performance benchmark comparing Ruff with other tools on the CPython codebase
- Microbenchmarks which run the linter or the formatter on individual files. These run on pull requests.
- Profiling the linter on either the microbenchmarks or entire projects

> **Note**
> When running benchmarks, ensure that your CPU is otherwise idle (e.g., close any background
> applications, like web browsers). You may also want to switch your CPU to a "performance"
> mode, if it exists, especially when benchmarking short-lived processes.

### CPython Benchmark

First, clone [CPython](https://github.com/python/cpython). It's a large and diverse Python codebase,
which makes it a good target for benchmarking.

```shell
git clone --branch 3.10 https://github.com/python/cpython.git crates/ruff_linter/resources/test/cpython
```

Install `hyperfine`:

```shell
cargo install hyperfine
```

To benchmark the release build:

```shell
cargo build --release --bin ruff && hyperfine --warmup 10 \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache -e" \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ -e"

Benchmark 1: ./target/release/ruff ./crates/ruff_linter/resources/test/cpython/ --no-cache
  Time (mean ± σ):     293.8 ms ±   3.2 ms    [User: 2384.6 ms, System: 90.3 ms]
  Range (min … max):   289.9 ms … 301.6 ms    10 runs

Benchmark 2: ./target/release/ruff ./crates/ruff_linter/resources/test/cpython/
  Time (mean ± σ):      48.0 ms ±   3.1 ms    [User: 65.2 ms, System: 124.7 ms]
  Range (min … max):    45.0 ms …  66.7 ms    62 runs

Summary
  './target/release/ruff ./crates/ruff_linter/resources/test/cpython/' ran
    6.12 ± 0.41 times faster than './target/release/ruff ./crates/ruff_linter/resources/test/cpython/ --no-cache'
```

To benchmark against the ecosystem's existing tools:

```shell
hyperfine --ignore-failure --warmup 5 \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache" \
  "pyflakes crates/ruff_linter/resources/test/cpython" \
  "autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython" \
  "pycodestyle crates/ruff_linter/resources/test/cpython" \
  "flake8 crates/ruff_linter/resources/test/cpython"

Benchmark 1: ./target/release/ruff ./crates/ruff_linter/resources/test/cpython/ --no-cache
  Time (mean ± σ):     294.3 ms ±   3.3 ms    [User: 2467.5 ms, System: 89.6 ms]
  Range (min … max):   291.1 ms … 302.8 ms    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 2: pyflakes crates/ruff_linter/resources/test/cpython
  Time (mean ± σ):     15.786 s ±  0.143 s    [User: 15.560 s, System: 0.214 s]
  Range (min … max):   15.640 s … 16.157 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 3: autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython
  Time (mean ± σ):      6.175 s ±  0.169 s    [User: 54.102 s, System: 1.057 s]
  Range (min … max):    5.950 s …  6.391 s    10 runs

Benchmark 4: pycodestyle crates/ruff_linter/resources/test/cpython
  Time (mean ± σ):     46.921 s ±  0.508 s    [User: 46.699 s, System: 0.202 s]
  Range (min … max):   46.171 s … 47.863 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 5: flake8 crates/ruff_linter/resources/test/cpython
  Time (mean ± σ):     12.260 s ±  0.321 s    [User: 102.934 s, System: 1.230 s]
  Range (min … max):   11.848 s … 12.933 s    10 runs

  Warning: Ignoring non-zero exit code.

Summary
  './target/release/ruff ./crates/ruff_linter/resources/test/cpython/ --no-cache' ran
   20.98 ± 0.62 times faster than 'autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython'
   41.66 ± 1.18 times faster than 'flake8 crates/ruff_linter/resources/test/cpython'
   53.64 ± 0.77 times faster than 'pyflakes crates/ruff_linter/resources/test/cpython'
  159.43 ± 2.48 times faster than 'pycodestyle crates/ruff_linter/resources/test/cpython'
```

To benchmark a subset of rules, e.g. `LineTooLong` and `DocLineTooLong`:

```shell
cargo build --release && hyperfine --warmup 10 \
  "./target/release/ruff check ./crates/ruff_linter/resources/test/cpython/ --no-cache -e --select W505,E501"
```

You can run `uv venv --project ./scripts/benchmarks`, activate the venv and then run `uv sync --project ./scripts/benchmarks` to create a working environment for the
above. All reported benchmarks were computed using the versions specified by
`./scripts/benchmarks/pyproject.toml` on Python 3.11.

To benchmark Pylint, remove the following files from the CPython repository:

```shell
rm Lib/test/bad_coding.py \
  Lib/test/bad_coding2.py \
  Lib/test/bad_getattr.py \
  Lib/test/bad_getattr2.py \
  Lib/test/bad_getattr3.py \
  Lib/test/badcert.pem \
  Lib/test/badkey.pem \
  Lib/test/badsyntax_3131.py \
  Lib/test/badsyntax_future10.py \
  Lib/test/badsyntax_future3.py \
  Lib/test/badsyntax_future4.py \
  Lib/test/badsyntax_future5.py \
  Lib/test/badsyntax_future6.py \
  Lib/test/badsyntax_future7.py \
  Lib/test/badsyntax_future8.py \
  Lib/test/badsyntax_future9.py \
  Lib/test/badsyntax_pep3120.py \
  Lib/test/test_asyncio/test_runners.py \
  Lib/test/test_copy.py \
  Lib/test/test_inspect.py \
  Lib/test/test_typing.py
```

Then, from `crates/ruff_linter/resources/test/cpython`, run: `time pylint -j 0 -E $(git ls-files '*.py')`. This
will execute Pylint with maximum parallelism and only report errors.

To benchmark Pyupgrade, run the following from `crates/ruff_linter/resources/test/cpython`:

```shell
hyperfine --ignore-failure --warmup 5 --prepare "git reset --hard HEAD" \
  "find . -type f -name \"*.py\" | xargs -P 0 pyupgrade --py311-plus"

Benchmark 1: find . -type f -name "*.py" | xargs -P 0 pyupgrade --py311-plus
  Time (mean ± σ):     30.119 s ±  0.195 s    [User: 28.638 s, System: 0.390 s]
  Range (min … max):   29.813 s … 30.356 s    10 runs
```

### Microbenchmarks

The `ruff_benchmark` crate benchmarks the linter and the formatter on individual files.

You can run the benchmarks with

```shell
cargo benchmark
```

`cargo benchmark` is an alias for `cargo bench -p ruff_benchmark --bench linter --bench formatter --`

#### Benchmark-driven Development

Ruff uses [Criterion.rs](https://bheisler.github.io/criterion.rs/book/) for benchmarks. You can use
`--save-baseline=<name>` to store an initial baseline benchmark (e.g., on `main`) and then use
`--benchmark=<name>` to compare against that benchmark. Criterion will print a message telling you
if the benchmark improved/regressed compared to that baseline.

```shell
# Run once on your "baseline" code
cargo bench -p ruff_benchmark -- --save-baseline=main

# Then iterate with
cargo bench -p ruff_benchmark -- --baseline=main
```

#### PR Summary

You can use `--save-baseline` and `critcmp` to get a pretty comparison between two recordings.
This is useful to illustrate the improvements of a PR.

```shell
# On main
cargo bench -p ruff_benchmark -- --save-baseline=main

# After applying your changes
cargo bench -p ruff_benchmark -- --save-baseline=pr

critcmp main pr
```

You must install [`critcmp`](https://github.com/BurntSushi/critcmp) for the comparison.

```bash
cargo install critcmp
```

#### Tips

- Use `cargo bench -p ruff_benchmark <filter>` to only run specific benchmarks. For example: `cargo bench -p ruff_benchmark lexer`
    to only run the lexer benchmarks.
- Use `cargo bench -p ruff_benchmark -- --quiet` for a more cleaned up output (without statistical relevance)
- Use `cargo bench -p ruff_benchmark -- --quick` to get faster results (more prone to noise)

### Profiling Projects

You can either use the microbenchmarks from above or a project directory for benchmarking. There
are a lot of profiling tools out there,
[The Rust Performance Book](https://nnethercote.github.io/perf-book/profiling.html) lists some
examples.

#### Linux

Install `perf` and build `ruff_benchmark` with the `profiling` profile and then run it with perf

```shell
cargo bench -p ruff_benchmark --no-run --profile=profiling && perf record --call-graph dwarf -F 9999 cargo bench -p ruff_benchmark --profile=profiling -- --profile-time=1
```

You can also use the `ruff_dev` launcher to run `ruff check` multiple times on a repository to
gather enough samples for a good flamegraph (change the 999, the sample rate, and the 30, the number
of checks, to your liking)

```shell
cargo build --bin ruff_dev --profile=profiling
perf record -g -F 999 target/profiling/ruff_dev repeat --repeat 30 --exit-zero --no-cache path/to/cpython > /dev/null
```

Then convert the recorded profile

```shell
perf script -F +pid > /tmp/test.perf
```

You can now view the converted file with [firefox profiler](https://profiler.firefox.com/). To learn more about Firefox profiler, read the [Firefox profiler profiling-guide](https://profiler.firefox.com/docs/#/./guide-perf-profiling).

An alternative is to convert the perf data to `flamegraph.svg` using
[flamegraph](https://github.com/flamegraph-rs/flamegraph) (`cargo install flamegraph`):

```shell
flamegraph --perfdata perf.data --no-inline
```

#### Mac

Install [`cargo-instruments`](https://crates.io/crates/cargo-instruments):

```shell
cargo install cargo-instruments
```

Then run the profiler with

```shell
cargo instruments -t time --bench linter --profile profiling -p ruff_benchmark -- --profile-time=1
```

- `-t`: Specifies what to profile. Useful options are `time` to profile the wall time and `alloc`
    for profiling the allocations.
- You may want to pass an additional filter to run a single test file

Otherwise, follow the instructions from the linux section.

## `cargo dev`

`cargo dev` is a shortcut for `cargo run --package ruff_dev --bin ruff_dev`. You can run some useful
utils with it:

- `cargo dev print-ast <file>`: Print the AST of a python file using Ruff's
    [Python parser](https://github.com/astral-sh/ruff/tree/main/crates/ruff_python_parser).
    For `if True: pass # comment`, you can see the syntax tree, the byte offsets for start and
    stop of each node and also how the `:` token, the comment and whitespace are not represented
    anymore:

```text
[
    If(
        StmtIf {
            range: 0..13,
            test: Constant(
                ExprConstant {
                    range: 3..7,
                    value: Bool(
                        true,
                    ),
                    kind: None,
                },
            ),
            body: [
                Pass(
                    StmtPass {
                        range: 9..13,
                    },
                ),
            ],
            orelse: [],
        },
    ),
]
```

- `cargo dev print-tokens <file>`: Print the tokens that the AST is built upon. Again for
    `if True: pass # comment`:

```text
0 If 2
3 True 7
7 Colon 8
9 Pass 13
14 Comment(
    "# comment",
) 23
23 Newline 24
```

- `cargo dev print-cst <file>`: Print the CST of a Python file using
    [LibCST](https://github.com/Instagram/LibCST), which is used in addition to the RustPython parser
    in Ruff. For example, for `if True: pass # comment`, everything, including the whitespace, is represented:

```text
Module {
    body: [
        Compound(
            If(
                If {
                    test: Name(
                        Name {
                            value: "True",
                            lpar: [],
                            rpar: [],
                        },
                    ),
                    body: SimpleStatementSuite(
                        SimpleStatementSuite {
                            body: [
                                Pass(
                                    Pass {
                                        semicolon: None,
                                    },
                                ),
                            ],
                            leading_whitespace: SimpleWhitespace(
                                " ",
                            ),
                            trailing_whitespace: TrailingWhitespace {
                                whitespace: SimpleWhitespace(
                                    " ",
                                ),
                                comment: Some(
                                    Comment(
                                        "# comment",
                                    ),
                                ),
                                newline: Newline(
                                    None,
                                    Real,
                                ),
                            },
                        },
                    ),
                    orelse: None,
                    leading_lines: [],
                    whitespace_before_test: SimpleWhitespace(
                        " ",
                    ),
                    whitespace_after_test: SimpleWhitespace(
                        "",
                    ),
                    is_elif: false,
                },
            ),
        ),
    ],
    header: [],
    footer: [],
    default_indent: "    ",
    default_newline: "\n",
    has_trailing_newline: true,
    encoding: "utf-8",
}
```

- `cargo dev generate-all`: Update `ruff.schema.json`, `docs/configuration.md` and `docs/rules`.
    You can also set `RUFF_UPDATE_SCHEMA=1` to update `ruff.schema.json` during `cargo test`.
- `cargo dev generate-cli-help`, `cargo dev generate-docs` and `cargo dev generate-json-schema`:
    Update just `docs/configuration.md`, `docs/rules` and `ruff.schema.json` respectively.
- `cargo dev generate-options`: Generate a markdown-compatible table of all `pyproject.toml`
    options. Used for <https://docs.astral.sh/ruff/settings/>.
- `cargo dev generate-rules-table`: Generate a markdown-compatible table of all rules. Used for <https://docs.astral.sh/ruff/rules/>.
- `cargo dev round-trip <python file or jupyter notebook>`: Read a Python file or Jupyter Notebook,
    parse it, serialize the parsed representation and write it back. Used to check how good our
    representation is so that fixes don't rewrite irrelevant parts of a file.
- `cargo dev format_dev`: See ruff_python_formatter README.md

## Subsystems

### Compilation Pipeline

If we view Ruff as a compiler, in which the inputs are paths to Python files and the outputs are
diagnostics, then our current compilation pipeline proceeds as follows:

1. **File discovery**: Given paths like `foo/`, locate all Python files in any specified subdirectories, taking into account our hierarchical settings system and any `exclude` options.

1. **Package resolution**: Determine the "package root" for every file by traversing over its parent directories and looking for `__init__.py` files.

1. **Cache initialization**: For every "package root", initialize an empty cache.

1. **Analysis**: For every file, in parallel:

    1. **Cache read**: If the file is cached (i.e., its modification timestamp hasn't changed since it was last analyzed), short-circuit, and return the cached diagnostics.

    1. **Tokenization**: Run the lexer over the file to generate a token stream.

    1. **Indexing**: Extract metadata from the token stream, such as: comment ranges, `# noqa` locations, `# isort: off` locations, "doc lines", etc.

    1. **Token-based rule evaluation**: Run any lint rules that are based on the contents of the token stream (e.g., commented-out code).

    1. **Filesystem-based rule evaluation**: Run any lint rules that are based on the contents of the filesystem (e.g., lack of `__init__.py` file in a package).

    1. **Logical line-based rule evaluation**: Run any lint rules that are based on logical lines (e.g., stylistic rules).

    1. **Parsing**: Run the parser over the token stream to produce an AST. (This consumes the token stream, so anything that relies on the token stream needs to happen before parsing.)

    1. **AST-based rule evaluation**: Run any lint rules that are based on the AST. This includes the vast majority of lint rules. As part of this step, we also build the semantic model for the current file as we traverse over the AST. Some lint rules are evaluated eagerly, as we iterate over the AST, while others are evaluated in a deferred manner (e.g., unused imports, since we can't determine whether an import is unused until we've finished analyzing the entire file), after we've finished the initial traversal.

    1. **Import-based rule evaluation**: Run any lint rules that are based on the module's imports (e.g., import sorting). These could, in theory, be included in the AST-based rule evaluation phase — they're just separated for simplicity.

    1. **Physical line-based rule evaluation**: Run any lint rules that are based on physical lines (e.g., line-length).

    1. **Suppression enforcement**: Remove any violations that are suppressed via `# noqa` directives or `per-file-ignores`.

    1. **Cache write**: Write the generated diagnostics to the package cache using the file as a key.

1. **Reporting**: Print diagnostics in the specified format (text, JSON, etc.), to the specified output channel (stdout, a file, etc.).

### Import Categorization

To understand Ruff's import categorization system, we first need to define two concepts:

- "Project root": The directory containing the `pyproject.toml`, `ruff.toml`, or `.ruff.toml` file,
    discovered by identifying the "closest" such directory for each Python file. (If you're running
    via `ruff --config /path/to/pyproject.toml`, then the current working directory is used as the
    "project root".)
- "Package root": The top-most directory defining the Python package that includes a given Python
    file. To find the package root for a given Python file, traverse up its parent directories until
    you reach a parent directory that doesn't contain an `__init__.py` file (and isn't in a subtree
    marked as a [namespace package](https://docs.astral.sh/ruff/settings/#namespace-packages)); take the directory
    just before that, i.e., the first directory in the package.

For example, given:

```text
my_project
├── pyproject.toml
└── src
    └── foo
        ├── __init__.py
        └── bar
            ├── __init__.py
            └── baz.py
```

Then when analyzing `baz.py`, the project root would be the top-level directory (`./my_project`),
and the package root would be `./my_project/src/foo`.

#### Project root

The project root does not have a significant impact beyond that all relative paths within the loaded
configuration file are resolved relative to the project root.

For example, to indicate that `bar` above is a namespace package (it isn't, but let's run with it),
the `pyproject.toml` would list `namespace-packages = ["./src/bar"]`, which would resolve
to `my_project/src/bar`.

The same logic applies when providing a configuration file via `--config`. In that case, the
_current working directory_ is used as the project root, and so all paths in that configuration file
are resolved relative to the current working directory. (As a general rule, we want to avoid relying
on the current working directory as much as possible, to ensure that Ruff exhibits the same behavior
regardless of where and how you invoke it — but that's hard to avoid in this case.)

Additionally, if a `pyproject.toml` file _extends_ another configuration file, Ruff will still use
the directory containing that `pyproject.toml` file as the project root. For example, if
`./my_project/pyproject.toml` contains:

```toml
[tool.ruff]
extend = "/path/to/pyproject.toml"
```

Then Ruff will use `./my_project` as the project root, even though the configuration file extends
`/path/to/pyproject.toml`. As such, if the configuration file at `/path/to/pyproject.toml` contains
any relative paths, they will be resolved relative to `./my_project`.

If a project uses nested configuration files, then Ruff would detect multiple project roots, one for
each configuration file.

#### Package root

The package root is used to determine a file's "module path". Consider, again, `baz.py`. In that
case, `./my_project/src/foo` was identified as the package root, so the module path for `baz.py`
would resolve to `foo.bar.baz` — as computed by taking the relative path from the package root
(inclusive of the root itself). The module path can be thought of as "the path you would use to
import the module" (e.g., `import foo.bar.baz`).

The package root and module path are used to, e.g., convert relative to absolute imports, and for
import categorization, as described below.

#### Import categorization

When sorting and formatting import blocks, Ruff categorizes every import into one of five
categories:

1. **"Future"**: the import is a `__future__` import. That's easy: just look at the name of the
    imported module!
1. **"Standard library"**: the import comes from the Python standard library (e.g., `import os`).
    This is easy too: we include a list of all known standard library modules in Ruff itself, so it's
    a simple lookup.
1. **"Local folder"**: the import is a relative import (e.g., `from .foo import bar`). This is easy
    too: just check if the import includes a `level` (i.e., a dot-prefix).
1. **"First party"**: the import is part of the current project. (More on this below.)
1. **"Third party"**: everything else.

The real challenge lies in determining whether an import is first-party — everything else is either
trivial, or (as in the case of third-party) merely defined as "not first-party".

There are three ways in which an import can be categorized as "first-party":

1. **Explicit settings**: the import is marked as such via the `known-first-party` setting. (This
    should generally be seen as an escape hatch.)
1. **Same-package**: the imported module is in the same package as the current file. This gets back
    to the importance of the "package root" and the file's "module path". Imagine that we're
    analyzing `baz.py` above. If `baz.py` contains any imports that appear to come from the `foo`
    package (e.g., `from foo import bar` or `import foo.bar`), they'll be classified as first-party
    automatically. This check is as simple as comparing the first segment of the current file's
    module path to the first segment of the import.
1. **Source roots**: Ruff supports a [`src`](https://docs.astral.sh/ruff/settings/#src) setting, which
    sets the directories to scan when identifying first-party imports. The algorithm is
    straightforward: given an import, like `import foo`, iterate over the directories enumerated in
    the `src` setting and, for each directory, check for the existence of a subdirectory `foo` or a
    file `foo.py`.

By default, `src` is set to the project root, along with `"src"` subdirectory in the project root.
This ensures that Ruff supports both flat and "src" layouts out of the box.
