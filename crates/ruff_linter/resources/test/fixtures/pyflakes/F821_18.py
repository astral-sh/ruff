"""Test bindings created within annotations."""

from typing import Annotated

foo = [1, 2, 3, 4, 5]


class Bar:
    # OK: Allow list comprehensions in annotations (i.e., treat `qux` as a valid
    # load in the scope of the annotation).
    baz: Annotated[
        str,
        [qux for qux in foo],
    ]


# OK: Allow named expressions in annotations.
x: (y := 1)
print(y)
