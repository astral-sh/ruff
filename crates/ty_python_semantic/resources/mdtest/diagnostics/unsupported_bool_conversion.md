<!-- snapshot-diagnostics -->

# Different ways that `unsupported-bool-conversion` can occur

## Has a `__bool__` method, but has incorrect parameters

```py
class NotBoolable:
    def __bool__(self, foo):
        return False

a = NotBoolable()

# error: [unsupported-bool-conversion]
10 and a and True
```

## Has a `__bool__` method, but has an incorrect return type

```py
class NotBoolable:
    def __bool__(self) -> str:
        return "wat"

a = NotBoolable()

# error: [unsupported-bool-conversion]
10 and a and True
```

## Has a `__bool__` attribute, but it's not callable

```py
class NotBoolable:
    __bool__: int = 3

a = NotBoolable()

# error: [unsupported-bool-conversion]
10 and a and True
```

## Part of a union where at least one member has incorrect `__bool__` method

```py
class NotBoolable1:
    def __bool__(self) -> str:
        return "wat"

class NotBoolable2:
    pass

class NotBoolable3:
    __bool__: int = 3

def get() -> NotBoolable1 | NotBoolable2 | NotBoolable3:
    return NotBoolable2()

# error: [unsupported-bool-conversion]
10 and get() and True
```
