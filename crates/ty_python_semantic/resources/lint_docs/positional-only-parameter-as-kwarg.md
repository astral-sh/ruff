## What it does

Checks for keyword arguments in calls that match positional-only parameters of the callable.

## Why is this bad?

Providing a positional-only parameter as a keyword argument will raise `TypeError` at runtime.

## Example

```python
def f(x: int, /) -> int:
    return x


f(x=1)  # error
```
