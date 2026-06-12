# Character-split notebook cell source

This is a regression test for <https://github.com/astral-sh/ty/issues/3525>. Some notebook tooling
stores cell source as an array containing one string per character instead of one string per line.

```ipynb
{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["p", "a", "s", "s", " ", " ", " "]
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": [
        "# snapshot\n",
        "x = 1  # ty: ignore[unresolved-reference]"
      ]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive
 --> src/mdtest_snippet.ipynb:cell 2:2:8
  |
2 | x = 1  # ty: ignore[unresolved-reference]
  |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove the unused suppression comment
 ::: cell 2
1 | # snapshot
  - x = 1  # ty: ignore[unresolved-reference]
2 + x = 1
```
