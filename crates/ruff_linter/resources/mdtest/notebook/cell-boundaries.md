# Notebook cell parsing

Ruff parses every notebook cell as its own module while retaining a single combined module for
linting.

## Syntax errors stop at cell boundaries

A decorator at the end of a cell would apply to a definition in the next cell if the sources were
concatenated. Parsing the cells independently must instead report the syntax error in the decorator's
cell.

`syntax-error.ipynb`:

```ipynb
{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["# snapshot\n", "@deco"]
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["def f(): pass"]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```

```snapshot
error[invalid-syntax]: Expected class, function definition or async function definition after decorator
 --> src/syntax-error.ipynb:cell 1:2:6
  |
2 | @deco
  |      ^
```

## Notebooks without Python cells

A notebook containing no Python code cells still parses successfully.

`no-code-cells.ipynb`:

```ipynb
{
  "cells": [
    {
      "cell_type": "markdown",
      "metadata": {},
      "source": ["# Nothing to check"]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```

## Cell boundary tokens

Per-cell parsing inserts tokens at each boundary before merging the cells into one module. This
notebook exercises a trailing `Dedent`, a definition referenced across cells, and a range suppression
whose directive and import are in separate cells. None of the selected rules should report a
diagnostic.

```toml
[lint]
select = [
  "E301",
  "E302",
  "E303",
  "E305",
  "E306",
  "F401",
  "F821",
  "W291",
  "W293",
  "W391",
]
```

`boundary-tokens.ipynb`:

```ipynb
{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["def compute():\n", "    return 1"]
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["# ruff: disable[F401]"]
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["import json\n", "print(compute())"]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```
