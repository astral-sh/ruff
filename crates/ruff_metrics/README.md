# Metrics

red-knot can generate metrics that describe its performance while type-checking Python code. To
activate metrics collection, pass the `--metrics` option when invoking red-knot:

```console
$ red_knot check --metrics [rest of arguments]
```

This will cause red-knot to _append_ metrics for the current run to a file called `metrics.json` in
the current directory.

You can then use the `plot_metrics.py` file to generate graphs of those metrics:

```console
$ uv run crates/ruff_metrics/plot_metrics.py counter semantic_index.scope_count --group-by file
```

## Available plots

### `counter`

Shows how the value of a counter increases over time. You can optionally group by one of the counter
metric's fields. Times are shown relative to the start of the process.

```console
$ uv run crates/ruff_metrics/plot_metrics.py counter semantic_index.scope_count --group-by file
```

### `histogram`

Shows the distribution of values of a counter. You must provide a metric field to group by; shows
the maxmimum values of the counter for each value of this field.

```console
$ uv run crates/ruff_metrics/plot_metrics.py histogram semantic_index.scope_count --group-by file
```

## Saving output to a file

You can save the plot to a file instead of displaying it by passing in the `-o` or `--output`
option:

```console
$ uv run crates/ruff_metrics/plot_metrics.py -o output.png counter semantic_index.scope_count --group-by file
```

(Note that the `--output` option must come before the subcommand selecting which kind of plot you
want.)

## Overriding the metrics file

You can optionally provide a filename for the `--metrics` option, in which case we will output
metrics data to that file instead of `./metrics.json`:

```console
$ red_knot check --metrics some-other-file.json [rest of arguments]
```

You can then pass the same filename to the `plot_metrics.py` script:

```console
$ uv run crates/ruff_metrics/plot_metrics.py --metrics some-other-file.json counter semantic_index.scope_count --group-by file
```

(Note that the `--metrics` option must come before the subcommand selecting which kind of plot you
want.)
