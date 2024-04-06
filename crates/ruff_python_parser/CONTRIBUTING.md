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
