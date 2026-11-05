## What it does

Checks for functions and methods without a return type annotation.

The rule applies even when ty can infer the return type. For overloaded functions, it checks the
overload signatures and skips the implementation.

When ty can infer an annotation from the implementation, it offers an unsafe fix. This works even
when return type inference is disabled for calls.

## Why is this bad?

A return annotation documents the result callers can expect and lets ty check that the
implementation returns a compatible value.

## Examples

```python
def double(value: int):  # error: [missing-return-annotation]
    return value * 2
```

Use instead:

```python
def double(value: int) -> int:
    return value * 2
```
