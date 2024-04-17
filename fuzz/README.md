# ruff-fuzz

Fuzzers and associated utilities for automatic testing of Ruff.

## Usage

To use the fuzzers provided in this directory, start by invoking:

```bash
./fuzz/init-fuzzers.sh
```

This will install [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) and optionally download a
[dataset](https://zenodo.org/record/3628784) which improves the efficacy of the testing.
**This step is necessary for initialising the corpus directory, as all fuzzers share a common
corpus.**
The dataset may take several hours to download and clean, so if you're just looking to try out the
fuzzers, skip the dataset download, though be warned that some features simply cannot be tested
without it (very unlikely for the fuzzer to generate valid python code from "thin air").

Once you have initialised the fuzzers, you can then execute any fuzzer with:

```bash
cargo fuzz run -s none name_of_fuzzer -- -timeout=1
```

**Users using Apple M1 devices must use a nightly compiler and omit the `-s none` portion of this
command, as this architecture does not support fuzzing without a sanitizer.**
You can view the names of the available fuzzers with `cargo fuzz list`.
For specific details about how each fuzzer works, please read this document in its entirety.

**IMPORTANT: You should run `./reinit-fuzzer.sh` after adding more file-based testcases.** This will
allow the testing of new features that you've added unit tests for.

### Debugging a crash

Once you've found a crash, you'll need to debug it.
The easiest first step in this process is to minimise the input such that the crash is still
triggered with a smaller input.
`cargo-fuzz` supports this out of the box with:

```bash
cargo fuzz tmin -s none name_of_fuzzer artifacts/name_of_fuzzer/crash-...
```

From here, you will need to analyse the input and potentially the behaviour of the program.
The debugging process from here is unfortunately less well-defined, so you will need to apply some
expertise here.
Happy hunting!

## A brief introduction to fuzzers

Fuzzing, or fuzz testing, is the process of providing generated data to a program under test.
The most common variety of fuzzers are mutational fuzzers; given a set of existing inputs (a
"corpus"), it will attempt to slightly change (or "mutate") these inputs into new inputs that cover
parts of the code that haven't yet been observed.
Using this strategy, we can quite efficiently generate testcases which cover significant portions of
the program, both with expected and unexpected data.
[This is really quite effective for finding bugs.](https://github.com/rust-fuzz/trophy-case)

The fuzzers here use [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz), a utility which allows
Rust to integrate with [libFuzzer](https://llvm.org/docs/LibFuzzer.html), the fuzzer library built
into LLVM.
Each source file present in [`fuzz_targets`](fuzz_targets) is a harness, which is, in effect, a unit
test which can handle different inputs.
When an input is provided to a harness, the harness processes this data and libFuzzer observes the
code coverage and any special values used in comparisons over the course of the run.
Special values are preserved for future mutations and inputs which cover new regions of code are
added to the corpus.

## Each fuzzer harness in detail

Each fuzzer harness in [`fuzz_targets`](fuzz_targets) targets a different aspect of Ruff and tests
them in different ways. While there is implementation-specific documentation in the source code
itself, each harness is briefly described below.

### `ruff_parse_simple`

This fuzz harness does not perform any "smart" testing of Ruff; it merely checks that the parsing
and unparsing of a particular input (what would normally be a source code file) does not crash.
It also attempts to verify that the locations of tokens and errors identified do not fall in the
middle of a UTF-8 code point, which may cause downstream panics.
While this is unlikely to find any issues on its own, it executes very quickly and covers a large
and diverse code region that may speed up the generation of inputs and therefore make a more
valuable corpus quickly.
It is particularly useful if you skip the dataset generation.

### `ruff_parse_idempotency`

This fuzz harness checks that Ruff's parser is idempotent in order to check that it is not
incorrectly parsing or unparsing an input.
It can be built in two modes: default (where it is only checked that the parser does not enter an
unstable state) or full idempotency (the parser is checked to ensure that it will _always_ produce
the same output after the first unparsing).
Full idempotency mode can be used by enabling the `full-idempotency` feature when running the
fuzzer, but this may be too strict of a restriction for initial testing.

### `ruff_fix_validity`

This fuzz harness checks that fixes applied by Ruff do not introduce new errors using the existing
[`ruff_linter::test::test_snippet`](../crates/ruff_linter/src/test.rs) testing utility.
It currently is only configured to use default settings, but may be extended in future versions to
test non-default linter settings.

### `ruff_formatter_idempotency`

This fuzz harness ensures that the formatter is [idempotent](https://en.wikipedia.org/wiki/Idempotence)
which detects possible unsteady states of Ruff's formatter.

### `ruff_formatter_validity`

This fuzz harness checks that Ruff's formatter does not introduce new linter errors/warnings by
linting once, counting the number of each error type, then formatting, then linting again and
ensuring that the number of each error type does not increase across formats. This has the
beneficial side effect of discovering cases where the linter does not discover a lint error when
it should have due to a formatting inconsistency.
