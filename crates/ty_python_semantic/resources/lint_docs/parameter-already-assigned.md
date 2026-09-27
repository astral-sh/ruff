## What it does

Checks for calls which provide more than one argument for a single parameter.

## Why is this bad?

Providing multiple values for a single parameter will raise a `TypeError` at runtime.

## Examples

```python
def f(x: int) -> int:
    return x


f(1, x=2)  # error
```
