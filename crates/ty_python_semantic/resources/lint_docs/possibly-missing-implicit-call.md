## What it does

Checks for implicit calls to possibly missing methods.

## Why is this bad?

Expressions such as `x[y] = z` and `del x[y]` call methods
under the hood (`__setitem__` and `__delitem__` respectively).
Calling a missing method will raise a `TypeError` at runtime.

## Examples

```python
import datetime


class A:
    if datetime.date.today().weekday() != 6:

        def __setitem__(self, key, value): ...


# TypeError: 'A' object does not support item assignment
A()[0] = 1  # error
```
