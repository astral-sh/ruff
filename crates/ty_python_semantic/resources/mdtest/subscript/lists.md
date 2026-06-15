# List subscripts

## Indexing into lists

A list can be indexed into with:

- numbers
- slices

```py
from typing import Any

x = [1, 2, 3]
reveal_type(x)  # revealed: list[int]

reveal_type(x[0])  # revealed: int

reveal_type(x[0:1])  # revealed: list[int]

# snapshot: invalid-argument-type
reveal_type(x["a"])  # revealed: Unknown

# error: [invalid-argument-type] "Cannot subscript an object of type `list[int]` with an index of type `Literal["b"]` (expected one of `SupportsIndex` or `slice[SupportsIndex | None, SupportsIndex | None, SupportsIndex | None]`)"
x["b"]

def invalid_slice_bound(xs: list[int], start: float) -> list[int]:
    return xs[start:]  # error: [invalid-argument-type]

def gradual_slice_bound(xs: list[int], start: Any) -> list[int]:
    return xs[start:]
```

```snapshot
error[invalid-argument-type]: Invalid subscript read
  --> src/mdtest_snippet.py:11:13
   |
11 | reveal_type(x["a"])  # revealed: Unknown
   |             -^^^^^
   |             | |
   |             | Has type `Literal["a"]`
   |             Has type `list[int]`
   |
info: This subscript expression implicitly calls `list[int].__getitem__`
info: First overload defined here
    --> stdlib/builtins.pyi:3029:5
     |
3029 | /     @overload
3030 | |     def __getitem__(self, i: SupportsIndex, /) -> _T:
3031 | |         """Return self[index]."""
     | |_________________________________^ First overload defined here
     |
info: Possible overloads for bound method `__getitem__`:
info:   (self, i: SupportsIndex, /) -> _T@list
info:   (self, s: slice[SupportsIndex | None, SupportsIndex | None, SupportsIndex | None], /) -> list[_T@list]
```

## Assignments within list assignment

In assignment, we might also have a named assignment. This should also get type checked.

```py
x = [1, 2, 3]
x[0 if (y := 2) else 1] = 5

# error: [invalid-assignment]
x["a" if (y := 2) else 1] = 6

# error: [invalid-assignment]
x["a" if (y := 2) else "b"] = 6

def invalid_slice_bound(xs: list[int], start: float) -> None:
    xs[start:] = []  # error: [invalid-assignment]
    del xs[start:]  # error: [invalid-argument-type]
```

## Walrus subscript access

```py
xs: list[int | None] = [1]
xs[0] = None

reveal_type((xs := [1])[0])  # revealed: int | None
```

## Walrus subscript access after rebinding

```py
def f(xs: list[int | str]) -> None:
    ys = xs
    ys[0] = "s"
    reveal_type((ys := [1])[0])  # revealed: int
```

## Walrus subscript access after later rebinding

```py
def f() -> None:
    (ys := [1])[0] = 2
    ys = ["s"]
    reveal_type(ys[0])  # revealed: str
```
