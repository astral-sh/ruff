# Contributing to the Python Parser

## Development

### Inline tests

The parser crate supports writing inline tests. These are tests that are written
in the source code itself, and are extracted to a separate file and run with the
test suite. They are written in the form of comments with a specific format. There
are two forms of inline tests:

Test that the parser successfully parses the input with no syntax errors. They're
written in the following format:

```rs
// test_ok this_is_the_test_name
// def foo():
//     pass
println!("some rust code");
```

Test that the parser fails to parse the input with a syntax error. They're written
in the following format:

```rs
// test_err this_is_the_test_name
// [1, 2
println!("some rust code");
```

Note that the difference between the two is the `test_ok` and `test_err` keywords.
The comment block must be independent of any other comment blocks. For example, the
following is not extracted:

```rs
// Some random comment
//
// test_ok this_is_the_test_name
// def foo():
//     pass
println!("some rust code");
```

To generate the corresponding Python files for the inline tests, run the following command:

```sh
cargo test --package ruff_python_parser --test generate_inline_tests
```

Then, run the Parser test suite with the following command:

```sh
cargo test --package ruff_python_parser
```

### Python-based fuzzer

The Ruff project includes a Python-based fuzzer that can be used to run the parser on
randomly generated (but syntactically valid) Python source code files.

To run the fuzzer, execute the following command
(requires [`uv`](https://github.com/astral-sh/uv) to be installed):

```sh
uvx --from ./python/py-fuzzer fuzz
```

Refer to the [py-fuzzer](https://github.com/astral-sh/ruff/blob/main/python/py-fuzzer/fuzz.py)
script for more information or use the `--help` flag to see the available options.

#### CI

The fuzzer is run as part of the CI pipeline. The purpose of running the fuzzer in the CI is to
catch any regressions introduced by any new changes to the parser. This is why the fuzzer is run on
the same set of seeds on every run.

## Benchmarks

The `ruff_benchmark` crate can benchmark both the lexer and the parser.

To run the lexer benchmarks, use the following command:

```sh
cargo bench --package ruff_benchmark --bench lexer
```

And to run the parser benchmarks, use the following command:

```sh
cargo bench --package ruff_benchmark --bench parser
```

Refer to the [Benchmarking and
Profiling](https://docs.astral.sh/ruff/contributing/#benchmark-driven-development) section in the
contributing guide for more information.
