## What it does

Checks for subscript assignments and deletions of read-only `TypedDict` items, including read-only
extra items. It also checks direct calls to `pop()` and `setdefault()` that could remove or insert a
read-only item.

## Why is this bad?

A `ReadOnly` annotation allows callers to pass a dictionary with a narrower item type. For example,
a caller could pass a movie whose title is always a string. Replacing the title with `None` would
violate that guarantee. Similarly, deleting an item could remove a key the caller requires.

## Example

```python
from typing_extensions import ReadOnly, TypedDict


class Movie(TypedDict):
    title: ReadOnly[str | None]


def clear_title(movie: Movie) -> None:
    movie["title"] = None  # error: [read-only-typed-dict-mutation]
```

## Migration

Before ty 0.0.97, subscript assignments to read-only items used `invalid-assignment`. Read-only
errors for `del`, `pop()`, and `setdefault()` used `invalid-argument-type`. To keep a custom rule
level for these errors, set `read-only-typed-dict-mutation` to that level as well. To suppress a
read-only mutation on a single line, use `# ty: ignore[read-only-typed-dict-mutation]`.
