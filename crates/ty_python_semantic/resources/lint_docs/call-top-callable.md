## What it does

Checks for calls to objects typed as `Top[Callable[..., T]]` (the infinite union of all
callable types with return type `T`).

## Why is this bad?

When an object is narrowed to `Top[Callable[..., object]]` (e.g., via `callable(x)` or
`isinstance(x, Callable)`), we know the object is callable, but we don't know its
precise signature. This type represents the set of all possible callable types
(including, e.g., functions that take no arguments and functions that require arguments),
so no specific set of arguments can be guaranteed to be valid.

## Examples

```python
def f(x: object):
    if callable(x):
        # We know `x` is callable, but not what arguments it accepts
        x()  # error
```
