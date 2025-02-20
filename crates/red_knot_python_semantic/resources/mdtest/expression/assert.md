## not-boolable condition

```py
class NotBoolable:
    __bool__ = 3

# error: [not-boolable] "Object of type `NotBoolable` has an invalid `__bool__` method"
assert NotBoolable()
```
