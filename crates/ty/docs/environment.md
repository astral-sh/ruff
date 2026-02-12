# Environment variables

ty defines and respects the following environment variables:

### `TY_CONFIG_FILE`

Path to a `ty.toml` configuration file to use.

When set, ty will use this file for configuration instead of
discovering configuration files automatically.

Equivalent to the `--config-file` command-line argument.

### `TY_LOG`

If set, ty will use this value as the log level for its `--verbose` output.
Accepts any filter compatible with the `tracing_subscriber` crate.

For example:

- `TY_LOG=ty=debug` is the equivalent of `-vv` to the command line
- `TY_LOG=trace` will enable all trace-level logging.

See the [tracing documentation](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#example-syntax)
for more.

### `TY_LOG_PROFILE`

If set to `"1"` or `"true"`, ty will enable flamegraph profiling.
This creates a `tracing.folded` file that can be used to generate flame graphs
for performance analysis.

### `TY_TDD_STATS_REPORT`

Controls reporting of TDD (ternary decision diagram) size statistics after `ty check`.

This is a developer-focused diagnostic mode and is only available when ty is built
with the `tdd-stats` cargo feature.
Without this feature, no TDD stats collection code is compiled into the binary.

Accepted values:

- `0`: Disable TDD stats output (default when unset).
- `1` or `short`: Emit summary and per-file counts through tracing target `ty.tdd_stats`.
  Includes both `reachability_*` and `narrowing_*` counters.
- `2` or `full`: Emit `1` output plus per-scope summaries (including histograms) and
  hot-node diagnostics through tracing target `ty.tdd_stats`.

Values greater than `2` are treated as `2` (`full`).

Example:

```bash
TY_TDD_STATS_REPORT=1 TY_LOG=ty.tdd_stats=info cargo run -p ty --features tdd-stats -- check path/to/project
```

```bash
TY_TDD_STATS_REPORT=2 TY_LOG=ty.tdd_stats=info cargo run -p ty --features tdd-stats -- check path/to/project
```

For tracing filter syntax and logging tips, see [Tracing](./tracing.md).

#### How to read `tdd_stats_summary` and `tdd_stats_file`

`short` and `full` both emit project-level and per-file summary lines on the `ty.tdd_stats` target:

```text
INFO tdd_stats_summary verbose=... files=... max_root_nodes=... reachability_roots=... reachability_nodes=... reachability_max_depth=... narrowing_roots=... narrowing_nodes=... narrowing_max_depth=...
INFO tdd_stats_file file=... max_root_nodes=... reachability_roots=... reachability_nodes=... reachability_max_depth=... narrowing_roots=... narrowing_nodes=... narrowing_max_depth=...
```

Field meanings:

- `verbose`: Effective verbosity level (`1` for short, `2` for full).
- `files`: Number of analyzed files with non-empty stats (summary line only).
- `max_root_nodes`: Largest interior-node count among single roots in scope.
- `reachability_roots` / `narrowing_roots`: Unique root-constraint ID counts split by family.
- `reachability_nodes` / `narrowing_nodes`: Interior-node visits split by family.
- `reachability_max_depth` / `narrowing_max_depth`: Maximum TDD depth observed in each family.

#### How to read `tdd_stats_hot_node` (full mode)

In `full` mode, ty emits `tdd_stats_hot_node` lines on the `ty.tdd_stats` target:

```text
INFO tdd_stats_hot_node file=... scope_id=... kind=... subtree_nodes=... root_uses=... score=... roots=...
```

Field meanings:

- `kind`: Which root family this hotspot is attributed to (`reachability` or `narrowing`).
- `subtree_nodes`: Number of interior nodes reachable from `constraint` (subtree size).
- `root_uses`: Number of root constraints whose TDD includes this interior node.
- `score`: Hotness score, computed as `subtree_nodes * root_uses`.
- `roots`: Up to five sample roots that include this node.
    - `line:column` means source location was resolved from an AST node.
    - `unknown` is fallback when source location could not be resolved.

Practical interpretation:

- Higher `score` means a larger subtree reused by many roots, hence a likely hotspot.
- If multiple top rows share very similar `roots`, they are often one clustered hotspot, not unrelated issues.
- Use `subtree_nodes` to spot deep/large structures and `root_uses` to spot broad fanout; both can dominate runtime.
- In `short` mode, compare `reachability_*` vs `narrowing_*` first to decide which family to investigate in `full` mode.

### `TY_MAX_PARALLELISM`

Specifies an upper limit for the number of tasks ty is allowed to run in parallel.

For example, how many files should be checked in parallel.
This isn't the same as a thread limit. ty may spawn additional threads
when necessary, e.g. to watch for file system changes or a dedicated UI thread.

### `TY_OUTPUT_FORMAT`

The format to use for printing diagnostic messages.

When set, ty will use this format for output instead of the default.

Accepts the same values as the `--output-format` command-line argument.

## Externally-defined variables

ty also reads the following externally defined environment variables:

### `CONDA_DEFAULT_ENV`

Used to determine the name of the active Conda environment.

### `CONDA_PREFIX`

Used to detect the path of an active Conda environment.
If both `VIRTUAL_ENV` and `CONDA_PREFIX` are present, `VIRTUAL_ENV` will be preferred.

### `PYTHONPATH`

Adds additional directories to ty's search paths.
The format is the same as the shell’s PATH:
one or more directory pathnames separated by os appropriate pathsep
(e.g. colons on Unix or semicolons on Windows).

### `RAYON_NUM_THREADS`

Specifies an upper limit for the number of threads ty uses when performing work in parallel.
Equivalent to `TY_MAX_PARALLELISM`.

This is a standard Rayon environment variable.

### `VIRTUAL_ENV`

Used to detect an activated virtual environment.

### `XDG_CONFIG_HOME`

Path to user-level configuration directory on Unix systems.

### `_CONDA_ROOT`

Used to determine the root install path of Conda.
