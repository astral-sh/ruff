## What it does

Checks for keyword arguments in calls that don't match any parameter of the callable.

## Why is this bad?

Providing an unknown keyword argument will raise `TypeError` at runtime.

## Example

```python
def f(x: int) -> int:
    return x


f(x=1, y=2)  # error
```
