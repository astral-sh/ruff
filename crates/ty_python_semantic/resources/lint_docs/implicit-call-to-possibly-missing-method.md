## What it does

Checks for implicit calls to possibly missing methods.

## Why is this bad?

Expressions such as `x[y]` and `x * y` call methods under the hood (`__getitem__` and `__mul__`
respectively). Calling a missing method will raise an `AttributeError` at runtime.

## Examples

```python
import datetime


class A:
    if datetime.date.today().weekday() != 6:

        def __getitem__(self, v): ...


# TypeError: 'A' object is not subscriptable
A()[0]  # error
```

## Rule history

- v0.0.102: Renamed from `possibly-missing-implicit-call`.

    `possibly-missing-implicit-call` remains as a compatibility alias for this rule.
