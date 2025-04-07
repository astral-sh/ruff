# Unary not

## None

```py
reveal_type(not None)  # revealed: Literal[True]
reveal_type(not not None)  # revealed: Literal[False]
```

## Function

```py
def f():
    return 1

reveal_type(not f)  # revealed: Literal[False]
# TODO Unknown should not be part of the type of typing.reveal_type
# reveal_type(not reveal_type)  revealed: Literal[False]
```

## Module

```py
import b
import warnings

reveal_type(not b)  # revealed: Literal[False]
reveal_type(not warnings)  # revealed: Literal[False]
```

`b.py`:

```py
y = 1
```

## Union

```py
def _(flag: bool):
    if flag:
        p = 1
        q = 3.3
        r = "hello"
        s = "world"
        t = 0
    else:
        p = "hello"
        q = 4
        r = ""
        s = 0
        t = ""

    reveal_type(not p)  # revealed: Literal[False]
    reveal_type(not q)  # revealed: bool
    reveal_type(not r)  # revealed: bool
    reveal_type(not s)  # revealed: bool
    reveal_type(not t)  # revealed: Literal[True]
```

## Integer literal

```py
reveal_type(not 1)  # revealed: Literal[False]
reveal_type(not 1234567890987654321)  # revealed: Literal[False]
reveal_type(not 0)  # revealed: Literal[True]
reveal_type(not -1)  # revealed: Literal[False]
reveal_type(not -1234567890987654321)  # revealed: Literal[False]
reveal_type(not --987)  # revealed: Literal[False]
```

## Boolean literal

```py
w = True
reveal_type(w)  # revealed: Literal[True]

x = False
reveal_type(x)  # revealed: Literal[False]

reveal_type(not w)  # revealed: Literal[False]

reveal_type(not x)  # revealed: Literal[True]
```

## String literal

```py
reveal_type(not "hello")  # revealed: Literal[False]
reveal_type(not "")  # revealed: Literal[True]
reveal_type(not "0")  # revealed: Literal[False]
reveal_type(not "hello" + "world")  # revealed: Literal[False]
```

## Bytes literal

```py
reveal_type(not b"hello")  # revealed: Literal[False]
reveal_type(not b"")  # revealed: Literal[True]
reveal_type(not b"0")  # revealed: Literal[False]
reveal_type(not b"hello" + b"world")  # revealed: Literal[False]
```

## Tuple

```py
reveal_type(not (1,))  # revealed: Literal[False]
reveal_type(not (1, 2))  # revealed: Literal[False]
reveal_type(not (1, 2, 3))  # revealed: Literal[False]
reveal_type(not ())  # revealed: Literal[True]
reveal_type(not ("hello",))  # revealed: Literal[False]
reveal_type(not (1, "hello"))  # revealed: Literal[False]
```

## Instance

Not operator is inferred based on
<https://docs.python.org/3/library/stdtypes.html#truth-value-testing>. An instance is True or False
if the `__bool__` method says so.

At runtime, the `__len__` method is a fallback for `__bool__`, but we can't make use of that. If we
have a class that defines `__len__` but not `__bool__`, it is possible that any subclass could add a
`__bool__` method that would invalidate whatever conclusion we drew from `__len__`. So instances of
classes without a `__bool__` method, with or without `__len__`, must be inferred as unknown
truthiness.

```py
from typing import Literal

class AlwaysTrue:
    def __bool__(self) -> Literal[True]:
        return True

# revealed: Literal[False]
reveal_type(not AlwaysTrue())

class AlwaysFalse:
    def __bool__(self) -> Literal[False]:
        return False

# revealed: Literal[True]
reveal_type(not AlwaysFalse())

# At runtime, no `__bool__` and no `__len__` means truthy, but we can't rely on that, because
# a subclass could add a `__bool__` method.
class NoBoolMethod: ...

# revealed: bool
reveal_type(not NoBoolMethod())

# And we can't rely on `__len__` for the same reason: a subclass could add `__bool__`.
class LenZero:
    def __len__(self) -> Literal[0]:
        return 0

# revealed: bool
reveal_type(not LenZero())

class LenNonZero:
    def __len__(self) -> Literal[1]:
        return 1

# revealed: bool
reveal_type(not LenNonZero())

class WithBothLenAndBool1:
    def __bool__(self) -> Literal[False]:
        return False

    def __len__(self) -> Literal[2]:
        return 2

# revealed: Literal[True]
reveal_type(not WithBothLenAndBool1())

class WithBothLenAndBool2:
    def __bool__(self) -> Literal[True]:
        return True

    def __len__(self) -> Literal[0]:
        return 0

# revealed: Literal[False]
reveal_type(not WithBothLenAndBool2())

class MethodBoolInvalid:
    def __bool__(self) -> int:
        return 0

# error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `MethodBoolInvalid`; the return type of its bool method (`int`) isn't assignable to `bool"
# revealed: bool
reveal_type(not MethodBoolInvalid())

# Don't trust a possibly-unbound `__bool__` method:
def get_flag() -> bool:
    return True

class PossiblyUnboundBool:
    if get_flag():
        def __bool__(self) -> Literal[False]:
            return False

# revealed: bool
reveal_type(not PossiblyUnboundBool())
```

## Object that implements `__bool__` incorrectly

<!-- snapshot-diagnostics -->

```py
class NotBoolable:
    __bool__: int = 3

# error: [unsupported-bool-conversion]
not NotBoolable()
```
