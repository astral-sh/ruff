# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
```

## List of tuples

```py
reveal_type([(1, 2), (3, 4)])  # revealed: list[tuple[int, int]]
```

## List of functions

```py
def a(_: int) -> int:
    return 0

def b(_: int) -> int:
    return 1

x = [a, b]
reveal_type(x)  # revealed: list[(_: int) -> int]
```

The inferred `Callable` type is function-like, i.e. we can still access attributes like `__name__`:

```py
reveal_type(x[0].__name__)  # revealed: str
```

## Mixed list

```py
# revealed: list[int | tuple[int, ...]]
reveal_type([1, (1, 2), (1, 2, 3)])
```

## Nested collection literals

Sibling collection literals share their element types when inferring the outer collection. This
avoids retaining a separate invariant collection type for every sibling.

Regression test for <https://github.com/astral-sh/ty/issues/3775>.

```py
from typing import Sequence

reveal_type([[1], ["a"]])  # revealed: list[list[int | str]]
reveal_type([[1], ["a"], [1, "a"]])  # revealed: list[list[int | str]]

# Existing collections retain their distinct invariant types.
ints = [1]
strings = ["a"]
reveal_type([ints, strings])  # revealed: list[list[int] | list[str]]

# Empty literals and singleton promotion are handled from the combined sibling contents.
reveal_type([[], [1]])  # revealed: list[list[int]]
reveal_type([[None], [1]])  # revealed: list[list[None | int]]

# The same inference applies to other nested collection kinds.
reveal_type([{1}, {"a"}])  # revealed: list[set[int | str]]
reveal_type([{"a": 1}, {"b": "x"}])  # revealed: list[dict[str, int | str]]
reveal_type({1: [1], 2: ["a"]})  # revealed: dict[int, list[int | str]]

# A useful covariant context takes precedence over peer simplification.
def accepts_separate_peers(value: Sequence[list[int] | list[str]]) -> None:
    pass

accepts_separate_peers([[1], ["a"]])

# Effects from peer pre-inference are produced exactly once by the real inference pass.
with_named_expression = [[(x := 1)], ["a"]]
reveal_type(with_named_expression)  # revealed: list[list[int | str]]
reveal_type(x)  # revealed: Literal[1]

with_diagnostic = [[1 + "x"], ["a"]]  # error: [unsupported-operator]

# A rejected union narrowing must not suppress diagnostics from the fallback inference.
# error: [unsupported-operator]
# error: [invalid-assignment]
invalid_union_context: list[list[int]] | None = [[1 + "x", "a"]]
```

## None promotion

`None` is promoted to `None | Unknown` in list literals when it is the only element type, so that
the inferred type does not overly restrict subsequent mutations of the list.

```py
from typing import Sequence

reveal_type([None])  # revealed: list[None | Unknown]
reveal_type([1, None])  # revealed: list[int | None]
reveal_type([(None,)])  # revealed: list[tuple[None | Unknown]]
reveal_type([[None], [None]])  # revealed: list[list[None | Unknown]]

x: list[int | None] = [None]
reveal_type(x)  # revealed: list[int | None]

y: list[tuple[int | None, ...]] = [(None,)]
reveal_type(y)  # revealed: list[tuple[int | None, ...]]

z: list[Sequence[int | str | None]] = [(None,), [None], (None, None)]
reveal_type(z)  # revealed: list[Sequence[int | str | None]]

xx: list[None] = reveal_type([None])  # revealed: list[None]
reveal_type(xx)  # revealed: list[None]

yy = reveal_type([None])  # revealed: list[None | Unknown]
reveal_type(yy)  # revealed: list[None | Unknown]

# Bare `list` in a type expression is equivalent to `list[Unknown]`
zz: list = [None]  # error: [missing-type-argument]
reveal_type(zz)  # revealed: list[Unknown]

# Promotion only happens if we're in invariant contexts,
# same as with `Literal` types:
reveal_type((1, 2, None))  # revealed: tuple[Literal[1], Literal[2], None]
reveal_type(((((None,),),),))  # revealed: tuple[tuple[tuple[tuple[None]]]]
reveal_type((((([None],),),),))  # revealed: tuple[tuple[tuple[tuple[list[None | Unknown]]]]]

class Foo:
    def __init__(self):
        self.mylist = [None, None, None]

    def method(self):
        self.mylist[0] = 42

reveal_type(Foo().mylist)  # revealed: list[None | Unknown]
```

## List comprehensions

```py
reveal_type([x for x in range(42)])  # revealed: list[int]
```
