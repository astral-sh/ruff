# Global Constants

## `__debug__` constant

The [`__debug__` constant] should be globally available:

```py
reveal_type(__debug__)  # revealed: bool

def foo():
    reveal_type(__debug__)  # revealed: bool
```

[`__debug__` constant]: https://docs.python.org/3/library/constants.html#debug__
