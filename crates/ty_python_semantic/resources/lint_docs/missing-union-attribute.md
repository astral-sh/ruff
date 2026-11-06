## What it does

Checks for reads or deletions of attributes that are present on some members of a union and missing
from others. If every member lacks the attribute, ty reports `missing-attribute`.

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

## Rule selection

Before ty 0.0.100, these accesses used the same rule as accesses to attributes missing from every
member. If you configured a severity such as `unresolved-attribute = "warn"`, also set
`missing-union-attribute = "warn"` to keep it for both cases. To suppress an access where only some
members lack the attribute, use `# ty: ignore[missing-union-attribute]`.
