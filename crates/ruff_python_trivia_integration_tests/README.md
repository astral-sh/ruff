# Integration tests for `ruff_python_trivia`

This crate includes integration tests for the `ruff_python_trivia` crate.

The reason for having a separate crate is to avoid introducing a dev circular
dependency between the `ruff_python_parser` crate and the `ruff_python_trivia` crate.

This crate shouldn't include any code, only tests.

**Reference:**

- `rust-analyzer` issue: <https://github.com/rust-lang/rust-analyzer/issues/3390>
- Ruff's pull request: <https://github.com/astral-sh/ruff/pull/11261>
