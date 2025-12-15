This directory contains a framework for evaluating completion suggestions
returned by the ty LSP.

# Running an evaluation

To run a full evaluation, run the `ty_completion_eval` crate with the
`all` command from the root of this repository:

```console
cargo run --profile profiling --package ty_completion_eval -- all
```

The output should look like this:

```text
    Finished `release` profile [optimized] target(s) in 0.09s
     Running `target/release/ty_completion_eval all`
mean reciprocal rank: 0.20409790112917506
MRR exceeds threshold of 0.001
```

If you want to look at the results of each individual evaluation task,
you can ask the evaluation to write CSV data that contains the rank of
the expected answer in each completion request:

```console
cargo r --profile profiling -p ty_completion_eval -- all --tasks ./crates/ty_completion_eval/completion-evaluation-tasks.csv
```

To debug a _specific_ task and look at the actual results, use the `show-one`
command:

```console
cargo r -q -p ty_completion_eval show-one higher-level-symbols-preferred --index 1
```

(The `--index` flag is only needed if there are multiple `<CURSOR>` directives in the same file.)

Has output that should look like this:

```text
ZQZQZQ_SOMETHING_IMPORTANT (*, 1/31)
__annotations__
__class__
__delattr__
__dict__
__dir__
__doc__
__eq__
__file__
__format__
__getattr__
__getattribute__
__getstate__
__hash__
__init__
__init_subclass__
__loader__
__module__
__name__
__ne__
__new__
__package__
__path__
__reduce__
__reduce_ex__
__repr__
__setattr__
__sizeof__
__spec__
__str__
__subclasshook__
-----
found 31 completions
```

The expected answer is marked with a `*`. The higher the rank, the better. In this example, the
rank is perfect. Note that the expected answer may not always appear in the completion results!
(Which is considered the worst possible outcome by this evaluation framework.)

# Evaluation model

This evaluation is based on [mean reciprocal rank] (MRR). That is, it assumes
that for every evaluation task (i.e., a single completion request) there is
precisely one correct answer. The higher the correct answer appears in each
completion request, the better. The mean reciprocal rank is computed as the
average of `1/rank` across all evaluation tasks. The higher the mean reciprocal
rank, the better.

The evaluation starts by preparing its truth data, which is contained in the `./truth` directory.
Within `./truth` is a list of Python projects. Every project contains one or more `<CURSOR>`
directives. Each `<CURSOR>` directive corresponds to an instruction to initiate a completion
request at that position. For example:

```python
class Foo:
    def frobnicate(self): pass

foo = Foo()
foo.frob<CURSOR: frobnicate>
```

The above example says that completions should be requested immediately after `foo.frob`
_and_ that the expected answer is `frobnicate`.

When testing auto-import, one should also include the module in the expected answer.
For example:

```python
RegexFl<CURSOR: re.RegexFlag>
```

Settings for completion requests can be configured via a `completion.toml` file within
each Python project directory.

When an evaluation is run, the truth data is copied to a temporary directory.
`uv sync` is then run within each directory to prepare it.

# Continuous Integration

At time of writing (2025-10-07), an evaluation is run in CI. CI will fail if the MRR is
below a set threshold. When this occurs, it means that the evaluation's results have likely
gotten worse in some measurable way. Ideally, the way to fix this would be to fix whatever
regression occurred in ranking. One can follow the steps above to run an evaluation and
emit the individual task results in CSV format. This difference between this CSV data and
whatever is committed at `./crates/ty_completion_eval/completion-evaluation-tasks.csv` should
point to where the regression occurs.

If the change is not a regression or is otherwise expected, then the MRR threshold can be
lowered. This requires changing how `ty_completion_eval` is executed within CI.

CI will also fail if the individual task results have changed.
To make CI pass, you can just re-run the evaluation locally and commit the results:

```console
cargo r --profile profiling -p ty_completion_eval -- all --tasks ./crates/ty_completion_eval/completion-evaluation-tasks.csv
```

CI fails in this case because it would be best to scrutinize the differences here.
It's possible that the ranking has improved in some measurable way, for example.
(Think of this as if it were a snapshot test.)

[mean reciprocal rank]: https://en.wikipedia.org/wiki/Mean_reciprocal_rank
