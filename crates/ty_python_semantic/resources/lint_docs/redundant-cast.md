## What it does

Detects redundant `cast` calls where the value already has the target type.

## Why is this bad?

These casts have no effect and can be removed.

## Example

```python
def f() -> int:
    return 10


cast(int, f())  # Redundant
```
