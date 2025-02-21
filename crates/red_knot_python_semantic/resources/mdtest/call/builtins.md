# Calling builtins

## `bool` with incorrect arguments

```py
class NotBool:
    __bool__ = None

# TODO: We should emit an `invalid-argument` error here for `2` because `bool` only takes one argument.
bool(1, 2)

# TODO: We should emit a `unsupported-bool-conversion` error here because the argument doesn't implement `__bool__` correctly.
bool(NotBool())
```
