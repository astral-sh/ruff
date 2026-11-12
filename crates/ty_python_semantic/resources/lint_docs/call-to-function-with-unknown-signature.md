## What it does

Detects calls to objects where ty knows that the object is callable, but cannot determine what the
signature of the callable object is.

Specifically, this diagnostic emits errors on calls to objects inferred as having the type
`Top[Callable[..., T]]`. This type represents the "infinite union" of all callable types with return
type `T`.

## Why is this bad?

When ty's `analysis.strict-generic-narrowing` setting is enabled, `if callable(x)` and
`if isinstance(x, Callable)` will narrow `x` to `Top[Callable[..., object]]`. We know that `x` is
callable, but we don't know its precise signature. `Top[Callable[..., object]]` represents the set
of all possible callable types (including, e.g., functions that take no arguments and functions that
require arguments), so no specific set of arguments can be guaranteed to be valid.

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

## Rule history

- v0.0.102: Renamed from `call-top-callable` to `call-to-function-with-unknown-signature`.

    `call-top-callable` remains as a compatibility alias for this rule.
