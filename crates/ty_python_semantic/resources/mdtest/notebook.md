# Notebook cells

Ruff and ty parse each notebook cell as its own module, so a syntax error confined to one cell is
surfaced instead of being masked by a later cell's content. The error is reported in the cell that
contains it, even when it is anchored at the cell's trailing end.

## Decorator at the end of a cell

A decorator must be immediately followed by a function or class definition. A cell ending in a
decorator is therefore invalid on its own, and the error is anchored at the separator after the
cell. It must still be reported in the decorator's cell rather than leaking into the next one.

```ipynb
{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["# snapshot\n", "@staticmethod"]
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["def f(): ..."]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```

```snapshot
error[invalid-syntax]: Expected class, function definition or async function definition after decorator
 --> src/mdtest_snippet.ipynb:cell 1:2:14
  |
2 | @staticmethod
  |              ^
```
