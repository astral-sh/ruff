## What it does

Checks for subscript reads of non-required `TypedDict` keys. A key can be non-required because it is
annotated with `NotRequired` or because its `TypedDict` declares `total=False`.

## Why is this bad?

Reading a missing key raises `KeyError` at runtime. Check for the key with `in` before reading it,
or use `get` to supply a default.

## Example

```python
from typing import NotRequired, TypedDict


class Movie(TypedDict):
    year: NotRequired[int]


def describe(movie: Movie) -> None:
    print(movie["year"])  # error

    if "year" in movie:
        print(movie["year"])

    print(movie.get("year", "unknown"))
```
