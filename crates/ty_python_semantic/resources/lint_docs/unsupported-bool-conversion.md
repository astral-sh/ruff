## What it does

Checks for bool conversions where the object doesn't correctly implement `__bool__`.

## Why is this bad?

If an exception is raised when you attempt to evaluate the truthiness of an object,
using the object in a boolean context will fail at runtime.

## Examples

```python
class NotBoolable:
    __bool__ = None


b1 = NotBoolable()
b2 = NotBoolable()

if b1:  # exception raised here
    pass

b1 and b2  # exception raised here
not b1  # exception raised here
b1 < b2 < b1  # exception raised here
```
