"""Test bindings created within annotations under `__future__` annotations."""

from __future__ import annotations

from typing import Annotated

foo = [1, 2, 3, 4, 5]


class Bar:
    # OK: Allow list comprehensions in annotations (i.e., treat `qux` as a valid
    # load in the scope of the annotation).
    baz: Annotated[
        str,
        [qux for qux in foo],
    ]


# Error: `y` is not defined.
x: (y := 1)
print(y)
