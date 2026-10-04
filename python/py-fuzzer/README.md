# py-fuzzer

A fuzzer script to run Ruff executables on randomly generated
(but syntactically valid) Python source-code files.

Run `uv run --project=./python/py-fuzzer fuzz -h` from the repository root
for more information and example invocations
(requires [`uv`](https://github.com/astral-sh/uv) to be installed).

Use `--output-format json` to write the final results as JSON to standard output; progress messages go to standard error. For example:

```shell
uv run --project=./python/py-fuzzer fuzz --bin ruff --output-format json 0-10 > results.json
```

The output contains a `bugs` array, ordered by seed. Each entry has a `seed` (a decimal string, so that large seeds can be read without losing precision), a `reproducer` containing Python source, and a `minimization_succeeded` boolean. When minimization fails or times out, the reproducer is the original generated source. If there are no findings, `bugs` is empty. The fuzzer exits with status 1 when it finds bugs and 0 when it finds none, as in the default text mode.
