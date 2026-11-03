## What it does

Checks for a function using one of its own local variables before it has a value or after its value
has been deleted.

## Why is this bad?

Reading or deleting an unbound local variable raises `UnboundLocalError` at runtime, even if a
global or builtin has the same name.

## Example

The assignment makes `count` local to `increment`, so `+=` tries to read its value before the
function has assigned one.

```python
count = 0


def increment():
    count += 1  # error
```

## Migration

Before ty 0.0.97, `unresolved-reference` also covered unbound locals. To keep a custom rule level
for both diagnostics, set `unbound-local` to the same level as `unresolved-reference`. On lines that
suppress an unbound local, change `ty: ignore[unresolved-reference]` to `ty: ignore[unbound-local]`.
List both codes if the line has both diagnostics.
