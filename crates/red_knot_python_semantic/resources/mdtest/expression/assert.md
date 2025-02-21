## Condition with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__ = 3

# error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `NotBoolable`; it incorrectly implements `__bool__`"
assert NotBoolable()
```
