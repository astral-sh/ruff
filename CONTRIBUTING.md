# Contributing to Ruff

Welcome! We're happy to have you here. Thank you in advance for your contribution to Ruff.

- [The Basics](#the-basics)
  - [Prerequisites](#prerequisites)
  - [Development](#development)
  - [Project Structure](#project-structure)
  - [Example: Adding a new lint rule](#example-adding-a-new-lint-rule)
    - [Rule naming convention](#rule-naming-convention)
    - [Rule testing: fixtures and snapshots](#rule-testing-fixtures-and-snapshots)
  - [Example: Adding a new configuration option](#example-adding-a-new-configuration-option)
- [MkDocs](#mkdocs)
- [Release Process](#release-process)
- [Benchmarks](#benchmarking-and-profiling)

## The Basics

Ruff welcomes contributions in the form of Pull Requests.

For small changes (e.g., bug fixes), feel free to submit a PR.

For larger changes (e.g., new lint rules, new functionality, new configuration options), consider
creating an [**issue**](https://github.com/astral-sh/ruff/issues) outlining your proposed change.
You can also join us on [**Discord**](https://discord.gg/c9MhzV8aU5) to discuss your idea with the
community.

If you're looking for a place to start, we recommend implementing a new lint rule (see:
[_Adding a new lint rule_](#example-adding-a-new-lint-rule), which will allow you to learn from and
pattern-match against the examples in the existing codebase. Many lint rules are inspired by
existing Python plugins, which can be used as a reference implementation.

As a concrete example: consider taking on one of the rules from the [`flake8-pyi`](https://github.com/astral-sh/ruff/issues/848)
plugin, and looking to the originating [Python source](https://github.com/PyCQA/flake8-pyi) for
guidance.

### Prerequisites

Ruff is written in Rust. You'll need to install the
[Rust toolchain](https://www.rust-lang.org/tools/install) for development.

You'll also need [Insta](https://insta.rs/docs/) to update snapshot tests:

```shell
cargo install cargo-insta
```

and pre-commit to run some validation checks:

```shell
pipx install pre-commit  # or `pip install pre-commit` if you have a virtualenv
```

### Development

After cloning the repository, run Ruff locally with:

```shell
cargo run -p ruff_cli -- check /path/to/file.py --no-cache
```

Prior to opening a pull request, ensure that your code has been auto-formatted,
and that it passes both the lint and test validation checks:

```shell
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Rust linting
RUFF_UPDATE_SCHEMA=1 cargo test  # Rust testing and updating ruff.schema.json
pre-commit run --all-files --show-diff-on-failure  # Rust and Python formatting, Markdown and Python linting, etc.
```

These checks will run on GitHub Actions when you open your Pull Request, but running them locally
will save you time and expedite the merge process.

Note that many code changes also require updating the snapshot tests, which is done interactively
after running `cargo test` like so:

```shell
cargo insta review
```

Your Pull Request will be reviewed by a maintainer, which may involve a few rounds of iteration
prior to merging.

### Project Structure

Ruff is structured as a monorepo with a [flat crate structure](https://matklad.github.io/2021/08/22/large-rust-workspaces.html),
such that all crates are contained in a flat `crates` directory.

The vast majority of the code, including all lint rules, lives in the `ruff` crate (located at
`crates/ruff`). As a contributor, that's the crate that'll be most relevant to you.

At time of writing, the repository includes the following crates:

- `crates/ruff`: library crate containing all lint rules and the core logic for running them.
- `crates/ruff_benchmark`: binary crate for running micro-benchmarks.
- `crates/ruff_cache`: library crate for caching lint results.
- `crates/ruff_cli`: binary crate containing Ruff's command-line interface.
- `crates/ruff_dev`: binary crate containing utilities used in the development of Ruff itself (e.g.,
  `cargo dev generate-all`).
- `crates/ruff_diagnostics`: library crate for the lint diagnostics APIs.
- `crates/ruff_formatter`: library crate for generic code formatting logic based on an intermediate
  representation.
- `crates/ruff_index`: library crate inspired by `rustc_index`.
- `crates/ruff_macros`: library crate containing macros used by Ruff.
- `crates/ruff_python_ast`: library crate containing Python-specific AST types and utilities.
- `crates/ruff_python_formatter`: library crate containing Python-specific code formatting logic.
- `crates/ruff_python_semantic`: library crate containing Python-specific semantic analysis logic,
  including Ruff's semantic model.
- `crates/ruff_python_stdlib`: library crate containing Python-specific standard library data.
- `crates/ruff_python_whitespace`: library crate containing Python-specific whitespace analysis
  logic.
- `crates/ruff_rustpython`: library crate containing `RustPython`-specific utilities.
- `crates/ruff_testing_macros`: library crate containing macros used for testing Ruff.
- `crates/ruff_textwrap`: library crate to indent and dedent Python source code.
- `crates/ruff_wasm`: library crate for exposing Ruff as a WebAssembly module.

### Example: Adding a new lint rule

At a high level, the steps involved in adding a new lint rule are as follows:

1. Determine a name for the new rule as per our [rule naming convention](#rule-naming-convention)
   (e.g., `AssertFalse`, as in, "allow `assert False`").

1. Create a file for your rule (e.g., `crates/ruff/src/rules/flake8_bugbear/rules/assert_false.rs`).

1. In that file, define a violation struct (e.g., `pub struct AssertFalse`). You can grep for
   `#[violation]` to see examples.

1. In that file, define a function that adds the violation to the diagnostic list as appropriate
   (e.g., `pub(crate) fn assert_false`) based on whatever inputs are required for the rule (e.g.,
   an `ast::StmtAssert` node).

1. Define the logic for triggering the violation in `crates/ruff/src/checkers/ast/mod.rs` (for
   AST-based checks), `crates/ruff/src/checkers/tokens.rs` (for token-based checks),
   `crates/ruff/src/checkers/lines.rs` (for text-based checks), or
   `crates/ruff/src/checkers/filesystem.rs` (for filesystem-based checks).

1. Map the violation struct to a rule code in `crates/ruff/src/codes.rs` (e.g., `B011`).

1. Add proper [testing](#rule-testing-fixtures-and-snapshots) for your rule.

1. Update the generated files (documentation and generated code).

To trigger the violation, you'll likely want to augment the logic in `crates/ruff/src/checkers/ast.rs`
to call your new function at the appropriate time and with the appropriate inputs. The `Checker`
defined therein is a Python AST visitor, which iterates over the AST, building up a semantic model,
and calling out to lint rule analyzer functions as it goes.

If you need to inspect the AST, you can run `cargo dev print-ast` with a Python file. Grep
for the `Diagnostic::new` invocations to understand how other, similar rules are implemented.

Once you're satisfied with your code, add tests for your rule. See [rule testing](#rule-testing-fixtures-and-snapshots)
for more details.

Finally, regenerate the documentation and other generated assets (like our JSON Schema) with:
`cargo dev generate-all`.

#### Rule naming convention

Like Clippy, Ruff's rule names should make grammatical and logical sense when read as "allow
${rule}" or "allow ${rule} items", as in the context of suppression comments.

For example, `AssertFalse` fits this convention: it flags `assert False` statements, and so a
suppression comment would be framed as "allow `assert False`".

As such, rule names should...

- Highlight the pattern that is being linted against, rather than the preferred alternative.
  For example, `AssertFalse` guards against `assert False` statements.

- _Not_ contain instructions on how to fix the violation, which instead belong in the rule
  documentation and the `autofix_title`.

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

1. Add a Python file to `crates/ruff/resources/test/fixtures/[linter]` that contains the code you
   want to test. The file name should match the rule name (e.g., `E402.py`), and it should include
   examples of both violations and non-violations.

1. Run Ruff locally against your file and verify the output is as expected. Once you're satisfied
   with the output (you see the violations you expect, and no others), proceed to the next step.
   For example, if you're adding a new rule named `E402`, you would run:

   ```shell
   cargo run -p ruff_cli -- check crates/ruff/resources/test/fixtures/pycodestyle/E402.py --no-cache
   ```

1. Add the test to the relevant `crates/ruff/src/rules/[linter]/mod.rs` file. If you're contributing
   a rule to a pre-existing set, you should be able to find a similar example to pattern-match
   against. If you're adding a new linter, you'll need to create a new `mod.rs` file (see,
   e.g., `crates/ruff/src/rules/flake8_bugbear/mod.rs`)

1. Run `cargo test`. Your test will fail, but you'll be prompted to follow-up
   with `cargo insta review`. Run `cargo insta review`, review and accept the generated snapshot,
   then commit the snapshot file alongside the rest of your changes.

1. Run `cargo test` again to ensure that your test passes.

### Example: Adding a new configuration option

Ruff's user-facing settings live in a few different places.

First, the command-line options are defined via the `Cli` struct in `crates/ruff/src/cli.rs`.

Second, the `pyproject.toml` options are defined in `crates/ruff/src/settings/options.rs` (via the
`Options` struct), `crates/ruff/src/settings/configuration.rs` (via the `Configuration` struct), and
`crates/ruff/src/settings/mod.rs` (via the `Settings` struct). These represent, respectively: the
schema used to parse the `pyproject.toml` file; an internal, intermediate representation; and the
final, internal representation used to power Ruff.

To add a new configuration option, you'll likely want to modify these latter few files (along with
`cli.rs`, if appropriate). If you want to pattern-match against an existing example, grep for
`dummy_variable_rgx`, which defines a regular expression to match against acceptable unused
variables (e.g., `_`).

Note that plugin-specific configuration options are defined in their own modules (e.g.,
`crates/ruff/src/flake8_unused_arguments/settings.rs`).

You may also want to add the new configuration option to the `flake8-to-ruff` tool, which is
responsible for converting `flake8` configuration files to Ruff's TOML format. This logic
lives in `crates/ruff/src/flake8_to_ruff/converter.rs`.

Finally, regenerate the documentation and generated code with `cargo dev generate-all`.

## MkDocs

To preview any changes to the documentation locally:

1. Install the [Rust toolchain](https://www.rust-lang.org/tools/install).

1. Install MkDocs and Material for MkDocs with:

   ```shell
   pip install -r docs/requirements.txt
   ```

1. Generate the MkDocs site with:

   ```shell
   python scripts/generate_mkdocs.py
   ```

1. Run the development server with:

   ```shell
   mkdocs serve
   ```

The documentation should then be available locally at
[http://127.0.0.1:8000/docs/](http://127.0.0.1:8000/docs/).

## Release Process

As of now, Ruff has an ad hoc release process: releases are cut with high frequency via GitHub
Actions, which automatically generates the appropriate wheels across architectures and publishes
them to [PyPI](https://pypi.org/project/ruff/).

Ruff follows the [semver](https://semver.org/) versioning standard. However, as pre-1.0 software,
even patch releases may contain [non-backwards-compatible changes](https://semver.org/#spec-item-4).

### Creating a new release

1. Update the version with `rg 0.0.269 --files-with-matches | xargs sed -i 's/0.0.269/0.0.270/g'`
1. Update `BREAKING_CHANGES.md`
1. Create a PR with the version and `BREAKING_CHANGES.md` updated
1. Merge the PR
1. Run the release workflow with the version number (without starting `v`) as input. Make sure
   main has your merged PR as last commit
1. The release workflow will do the following:
   1. Build all the assets. If this fails (even though we tested in step 4), we haven’t tagged or
      uploaded anything, you can restart after pushing a fix
   1. Upload to pypi
   1. Create and push the git tag (from pyproject.toml). We create the git tag only here
      because we can't change it ([#4468](https://github.com/charliermarsh/ruff/issues/4468)), so
      we want to make sure everything up to and including publishing to pypi worked.
   1. Attach artifacts to draft GitHub release
   1. Trigger downstream repositories. This can fail without causing fallout, it is possible (if
      inconvenient) to trigger the downstream jobs manually
1. Create release notes in GitHub UI and promote from draft to proper release(<https://github.com/charliermarsh/ruff/releases/new>)
1. If needed, [update the schemastore](https://github.com/charliermarsh/ruff/blob/main/scripts/update_schemastore.py)
1. If needed, update ruff-lsp and ruff-vscode

## Ecosystem CI

GitHub Actions will run your changes against a number of real-world projects from GitHub and
report on any diagnostic differences. You can also run those checks locally via:

```shell
python scripts/check_ecosystem.py path/to/your/ruff path/to/older/ruff
```

You can also run the Ecosystem CI check in a Docker container across a larger set of projects by
downloading the [`known-github-tomls.json`](https://github.com/akx/ruff-usage-aggregate/blob/master/data/known-github-tomls.jsonl)
as `github_search.jsonl` and following the instructions in [scripts/Dockerfile.ecosystem](https://github.com/astral-sh/ruff/blob/main/scripts/Dockerfile.ecosystem).
Note that this check will take a while to run.

## Benchmarking and Profiling

We have several ways of benchmarking and profiling Ruff:

- Our main performance benchmark comparing Ruff with other tools on the CPython codebase
- Microbenchmarks which the linter or the formatter on individual files. There run on pull requests.
- Profiling the linter on either the microbenchmarks or entire projects

### CPython Benchmark

First, clone [CPython](https://github.com/python/cpython). It's a large and diverse Python codebase,
which makes it a good target for benchmarking.

```shell
git clone --branch 3.10 https://github.com/python/cpython.git crates/ruff/resources/test/cpython
```

To benchmark the release build:

```shell
cargo build --release && hyperfine --warmup 10 \
  "./target/release/ruff ./crates/ruff/resources/test/cpython/ --no-cache -e" \
  "./target/release/ruff ./crates/ruff/resources/test/cpython/ -e"

Benchmark 1: ./target/release/ruff ./crates/ruff/resources/test/cpython/ --no-cache
  Time (mean ± σ):     293.8 ms ±   3.2 ms    [User: 2384.6 ms, System: 90.3 ms]
  Range (min … max):   289.9 ms … 301.6 ms    10 runs

Benchmark 2: ./target/release/ruff ./crates/ruff/resources/test/cpython/
  Time (mean ± σ):      48.0 ms ±   3.1 ms    [User: 65.2 ms, System: 124.7 ms]
  Range (min … max):    45.0 ms …  66.7 ms    62 runs

Summary
  './target/release/ruff ./crates/ruff/resources/test/cpython/' ran
    6.12 ± 0.41 times faster than './target/release/ruff ./crates/ruff/resources/test/cpython/ --no-cache'
```

To benchmark against the ecosystem's existing tools:

```shell
hyperfine --ignore-failure --warmup 5 \
  "./target/release/ruff ./crates/ruff/resources/test/cpython/ --no-cache" \
  "pyflakes crates/ruff/resources/test/cpython" \
  "autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython" \
  "pycodestyle crates/ruff/resources/test/cpython" \
  "flake8 crates/ruff/resources/test/cpython"

Benchmark 1: ./target/release/ruff ./crates/ruff/resources/test/cpython/ --no-cache
  Time (mean ± σ):     294.3 ms ±   3.3 ms    [User: 2467.5 ms, System: 89.6 ms]
  Range (min … max):   291.1 ms … 302.8 ms    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 2: pyflakes crates/ruff/resources/test/cpython
  Time (mean ± σ):     15.786 s ±  0.143 s    [User: 15.560 s, System: 0.214 s]
  Range (min … max):   15.640 s … 16.157 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 3: autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython
  Time (mean ± σ):      6.175 s ±  0.169 s    [User: 54.102 s, System: 1.057 s]
  Range (min … max):    5.950 s …  6.391 s    10 runs

Benchmark 4: pycodestyle crates/ruff/resources/test/cpython
  Time (mean ± σ):     46.921 s ±  0.508 s    [User: 46.699 s, System: 0.202 s]
  Range (min … max):   46.171 s … 47.863 s    10 runs

  Warning: Ignoring non-zero exit code.

Benchmark 5: flake8 crates/ruff/resources/test/cpython
  Time (mean ± σ):     12.260 s ±  0.321 s    [User: 102.934 s, System: 1.230 s]
  Range (min … max):   11.848 s … 12.933 s    10 runs

  Warning: Ignoring non-zero exit code.

Summary
  './target/release/ruff ./crates/ruff/resources/test/cpython/ --no-cache' ran
   20.98 ± 0.62 times faster than 'autoflake --recursive --expand-star-imports --remove-all-unused-imports --remove-unused-variables --remove-duplicate-keys resources/test/cpython'
   41.66 ± 1.18 times faster than 'flake8 crates/ruff/resources/test/cpython'
   53.64 ± 0.77 times faster than 'pyflakes crates/ruff/resources/test/cpython'
  159.43 ± 2.48 times faster than 'pycodestyle crates/ruff/resources/test/cpython'
```

You can run `poetry install` from `./scripts/benchmarks` to create a working environment for the
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

Then, from `crates/ruff/resources/test/cpython`, run: `time pylint -j 0 -E $(git ls-files '*.py')`. This
will execute Pylint with maximum parallelism and only report errors.

To benchmark Pyupgrade, run the following from `crates/ruff/resources/test/cpython`:

```shell
hyperfine --ignore-failure --warmup 5 --prepare "git reset --hard HEAD" \
  "find . -type f -name \"*.py\" | xargs -P 0 pyupgrade --py311-plus"

Benchmark 1: find . -type f -name "*.py" | xargs -P 0 pyupgrade --py311-plus
  Time (mean ± σ):     30.119 s ±  0.195 s    [User: 28.638 s, System: 0.390 s]
  Range (min … max):   29.813 s … 30.356 s    10 runs
```

## Microbenchmarks

The `ruff_benchmark` crate benchmarks the linter and the formatter on individual files.

You can run the benchmarks with

```shell
cargo benchmark
```

### Benchmark driven Development

Ruff uses [Criterion.rs](https://bheisler.github.io/criterion.rs/book/) for benchmarks. You can use
`--save-baseline=<name>` to store an initial baseline benchmark (e.g. on `main`) and then use
`--benchmark=<name>` to compare against that benchmark. Criterion will print a message telling you
if the benchmark improved/regressed compared to that baseline.

```shell
# Run once on your "baseline" code
cargo benchmark --save-baseline=main

# Then iterate with
cargo benchmark --baseline=main
```

### PR Summary

You can use `--save-baseline` and `critcmp` to get a pretty comparison between two recordings.
This is useful to illustrate the improvements of a PR.

```shell
# On main
cargo benchmark --save-baseline=main

# After applying your changes
cargo benchmark --save-baseline=pr

critcmp main pr
```

You must install [`critcmp`](https://github.com/BurntSushi/critcmp) for the comparison.

```bash
cargo install critcmp
```

### Tips

- Use `cargo benchmark <filter>` to only run specific benchmarks. For example: `cargo benchmark linter/pydantic`
  to only run the pydantic tests.
- Use `cargo benchmark --quiet` for a more cleaned up output (without statistical relevance)
- Use `cargo benchmark --quick` to get faster results (more prone to noise)

## Profiling Projects

You can either use the microbenchmarks from above or a project directory for benchmarking. There
are a lot of profiling tools out there,
[The Rust Performance Book](https://nnethercote.github.io/perf-book/profiling.html) lists some
examples.

### Linux

Install `perf` and build `ruff_benchmark` with the `release-debug` profile and then run it with perf

```shell
cargo bench -p ruff_benchmark --no-run --profile=release-debug && perf record --call-graph dwarf -F 9999 cargo bench -p ruff_benchmark --profile=release-debug -- --profile-time=1
```

You can also use the `ruff_dev` launcher to run `ruff check` multiple times on a repository to
gather enough samples for a good flamegraph (change the 999, the sample rate, and the 30, the number
of checks, to your liking)

```shell
cargo build --bin ruff_dev --profile=release-debug
perf record -g -F 999 target/release-debug/ruff_dev repeat --repeat 30 --exit-zero --no-cache path/to/cpython > /dev/null
```

Then convert the recorded profile

```shell
perf script -F +pid > /tmp/test.perf
```

You can now view the converted file with [firefox profiler](https://profiler.firefox.com/), with a
more in-depth guide [here](https://profiler.firefox.com/docs/#/./guide-perf-profiling)

An alternative is to convert the perf data to `flamegraph.svg` using
[flamegraph](https://github.com/flamegraph-rs/flamegraph) (`cargo install flamegraph`):

```shell
flamegraph --perfdata perf.data
```

### Mac

Install [`cargo-instruments`](https://crates.io/crates/cargo-instruments):

```shell
cargo install cargo-instruments
```

Then run the profiler with

```shell
cargo instruments -t time --bench linter --profile release-debug -p ruff_benchmark -- --profile-time=1
```

- `-t`: Specifies what to profile. Useful options are `time` to profile the wall time and `alloc`
  for profiling the allocations.
- You may want to pass an additional filter to run a single test file

Otherwise, follow the instructions from the linux section.
