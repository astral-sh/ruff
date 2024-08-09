# Ruff Benchmarks

The `ruff_benchmark` crate benchmarks the linter and the formatter on individual files:

```shell
# Run once on the "baseline".
cargo bench -p ruff_benchmark -- --save-baseline=main

# Compare against the "baseline".
cargo bench -p ruff_benchmark -- --baseline=main

# Run the lexer benchmarks.
cargo bench -p ruff_benchmark lexer -- --baseline=main
```

See [CONTRIBUTING.md](../../CONTRIBUTING.md) on how to use these benchmarks.
