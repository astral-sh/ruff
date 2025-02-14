## not-boolable condition

```py
class NotBoolable:
    __bool__ = 3

# error: [not-boolable] "Object of type `NotBoolable` can not be converted to a bool."
assert NotBoolable()
```
