# Ruff Micro-benchmarks

Benchmarks for the different Ruff-tools.

## Run Benchmark

You can run the benchmarks with

```shell
cargo benchmark
```

## Benchmark driven Development

You can use `--save-baseline=<name>` to store an initial baseline benchmark (e.g. on `main`) and
then use `--benchmark=<name>` to compare against that benchmark. Criterion will print a message
telling you if the benchmark improved/regressed compared to that baseline.

```shell
# Run once on your "baseline" code
cargo benchmark --save-baseline=main

# Then iterate with
cargo benchmark --baseline=main
```

## PR Summary

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

## Tips

- Use `cargo benchmark <filter>` to only run specific benchmarks. For example: `cargo benchmark linter/pydantic`
  to only run the pydantic tests.
- Use `cargo benchmark --quiet` for a more cleaned up output (without statistical relevance)
- Use `cargo benchmark --quick` to get faster results (more prone to noise)

## Profiling

### Linux

Install `perf` and build `ruff_benchmark` with the `release-debug` profile and then run it with perf

```shell
cargo bench -p ruff_benchmark --no-run --profile=release-debug && perf record -g -F 9999 cargo bench -p ruff_benchmark --profile=release-debug -- --profile-time=1
```

Then convert the recorded profile

```shell
perf script -F +pid > /tmp/test.perf
```

You can now view the converted file with [firefox profiler](https://profiler.firefox.com/)

You can find a more in-depth guide [here](https://profiler.firefox.com/docs/#/./guide-perf-profiling)

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
