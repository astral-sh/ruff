## What it does

Reports attempts to subscript `TypedDict` instances with a key of type `str`.

## Why is this bad?

A `TypedDict` declares a mapping type where a known set of keys may exist and each key may have a
distinct value type. Given an arbitrary `str`, ty cannot check for possible `KeyError`s, and cannot
infer a precise type for the subscript expression.

## Example

```python
from typing import TypedDict


class Movie(TypedDict):
    title: str
    year: int


def get_field(movie: Movie, key: str) -> str | int:
    return movie[key]  # error: [non-literal-typed-dict-key]
```

Use instead:

```py
from typing import Literal


def get_known_field(movie: Movie, key: Literal["title", "year"]) -> str | int:
    return movie[key]
```

## See also

- `invalid-typed-dict-key` reports other invalid subscripts, such as a misspelled field name or an
    integer key.
