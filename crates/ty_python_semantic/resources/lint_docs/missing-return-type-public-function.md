## What it does

Checks for public functions and methods without a return type annotation.

## Why is this bad?

A return annotation documents the result callers can expect and lets ty check that the
implementation returns a compatible value.

## Examples

```python
def double(value: int):  # error: [missing-return-type-public-function]
    return value * 2
```

Use instead:

```python
def double(value: int) -> int:
    return value * 2
```
