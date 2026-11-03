## What it does

Reports a string key when a `TypedDict` subscript requires a known key and ty cannot narrow the key
to a string literal or a union of string literals.

## Why is this bad?

Each declared `TypedDict` key has its own value type. Given an arbitrary `str`, ty cannot check
whether the key exists or which value type applies.

## Example

```python
from typing import Literal, TypedDict


class Movie(TypedDict):
    title: str
    year: int


def get_field(movie: Movie, key: str) -> str | int:
    return movie[key]  # error: [non-literal-typeddict-key]
```

Use instead:

```py
def get_known_field(movie: Movie, key: Literal["title", "year"]) -> str | int:
    return movie[key]
```

## See also

- `invalid-typeddict-key` reports invalid keys such as a misspelled field name or an integer
    subscript.
