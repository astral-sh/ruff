## Condition with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__: int = 3

# error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `NotBoolable`; its `__bool__` method isn't callable"
assert NotBoolable()
```
