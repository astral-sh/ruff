# Running `mypy_primer`

## Basics

For now, we use our own [fork of mypy primer]. It can be run using `uvx --from "…" mypy_primer`. For example, to see the help message, run:

```sh
uvx --from "git+https://github.com/astral-sh/mypy_primer.git@add-red-knot-support" mypy_primer -h
```

Alternatively, you can install the forked version of `mypy_primer` using:

```sh
uv tool install "git+https://github.com/astral-sh/mypy_primer.git@add-red-knot-support"
```

and then run it using `uvx mypy_primer` or just `mypy_primer`, if your `PATH` is set up accordingly (see: [Tool executables]).

## Showing the diagnostics diff between two Git revisions

To show the diagnostics diff between two Git revisions (e.g. your feature branch and `main`), run:

```sh
mypy_primer \
    --type-checker knot \
    --old origin/main \
    --new my/feature \
    --debug \
    --output concise \
    --project-selector '/black$'
```

This will show the diagnostics diff for the `black` project between the `main` branch and your `my/feature` branch. To run the
diff for all projects, you currently need to copy the project-selector regex from the CI pipeline in `.github/workflows/mypy_primer.yaml`.

You can also take a look at the [full list of ecosystem projects]. Note that some of them might still need a `knot_paths` configuration
option to work correctly.

## Avoiding recompilation

If you want to run `mypy_primer` repeatedly, e.g. for different projects, but for the same combination of `--old` and `--new`, you
can use set the `MYPY_PRIMER_NO_REBUILD` environment variable to avoid recompilation of Red Knot:

```sh
MYPY_PRIMER_NO_REBUILD=1 mypy_primer …
```

## Running from a local copy of the repository

If you are working on a local branch, you can use `mypy_primer`'s `--repo` option to specify the path to your local copy of the `ruff` repository.
This allows `mypy_primer` to check out local branches:

```sh
mypy_primer --repo /path/to/ruff --old origin/main --new my/local-branch …
```

Note that you might need to clean up `/tmp/mypy_primer` in order for this to work correctly.

[fork of mypy primer]: https://github.com/astral-sh/mypy_primer/tree/add-red-knot-support
[full list of ecosystem projects]: https://github.com/astral-sh/mypy_primer/blob/add-red-knot-support/mypy_primer/projects.py
[tool executables]: https://docs.astral.sh/uv/concepts/tools/#tool-executables
