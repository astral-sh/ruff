## What it does

Checks for reads or deletions of attributes that are present on some members of a union and missing
from others.

## Why is this bad?

Reading or deleting the attribute raises an `AttributeError` when the runtime value lacks it.

## Examples

```python
def normalize(value: str | bytes) -> str:
    return value.decode()  # error: [missing-union-attribute]
```

Narrow the type before accessing the attribute:

```python
def normalize(value: str | bytes) -> str:
    if isinstance(value, bytes):
        return value.decode()
    return value
```

## See also

- `missing-attribute` reports attributes missing from every union member and explains how to migrate
    from `unresolved-attribute`.
