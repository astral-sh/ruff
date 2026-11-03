## What it does

Checks for calls to objects typed as `Top[Callable[..., T]]` (the infinite union of all callable
types with return type `T`).

## Why is this bad?

When `analysis.strict-generic-narrowing` is enabled, `callable(x)` and `isinstance(x, Callable)`
narrow an object to `Top[Callable[..., object]]`. We know the object is callable, but we don't know
its precise signature. This type represents the set of all possible callable types (including, e.g.,
functions that take no arguments and functions that require arguments), so no specific set of
arguments can be guaranteed to be valid.

## Examples

```toml
[analysis]
strict-generic-narrowing = true
```

```python
def f(x: object):
    if callable(x):
        # We know `x` is callable, but not what arguments it accepts
        x()  # error
```
