## What it does

Checks for calls to `super()` inside methods of `NamedTuple` classes.

## Why is this bad?

Using `super()` in a method of a `NamedTuple` class will raise an exception at runtime.

## Examples

```python
from typing import NamedTuple


class F(NamedTuple):
    x: int

    def method(self):
        # super() is not supported in methods of NamedTuple classes
        super()  # error
```

## References

- [Python documentation: super()](https://docs.python.org/3/library/functions.html#super)
