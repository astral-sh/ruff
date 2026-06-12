## What it does

Checks for calls which provide more than one argument for a single parameter.

## Why is this bad?

Providing multiple values for a single parameter will raise a `TypeError` at runtime.

## Examples

```python
def f(x: int) -> int: ...


f(1, x=2)  # Error raised here
```
