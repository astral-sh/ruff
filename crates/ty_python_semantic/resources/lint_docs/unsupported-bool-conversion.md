## What it does

Checks for bool conversions where the object doesn't correctly implement `__bool__`.

## Why is this bad?

If an exception is raised when you attempt to evaluate the truthiness of an object,
using the object in a boolean context will fail at runtime.

## Examples

```python
class NotBoolable:
    __bool__ = None

    def __lt__(self, other: object) -> "NotBoolable":
        return self


b1 = NotBoolable()
b2 = NotBoolable()

# exception raised here
if b1:  # error
    pass

# exception raised here
b1 and b2  # error
# exception raised here
not b1  # error

# A chained comparison converts the result of `b1 < b2` to bool.
# exception raised here
b1 < b2 < b1  # error
```
