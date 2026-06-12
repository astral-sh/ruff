## What it does

Checks for implicit calls to possibly missing methods.

## Why is this bad?

Expressions such as `x[y]` and `x * y` call methods
under the hood (`__getitem__` and `__mul__` respectively).
Calling a missing method will raise an `AttributeError` at runtime.

## Examples

```python
import datetime


class A:
    if datetime.date.today().weekday() != 6:

        def __getitem__(self, v): ...


# TypeError: 'A' object is not subscriptable
A()[0]  # error
```
