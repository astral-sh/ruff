# Expressions

## OR

```py
def _(foo: str):
    reveal_type(True or False)  # revealed: Literal[True]
    reveal_type("x" or "y" or "z")  # revealed: Literal["x"]
    reveal_type("" or "y" or "z")  # revealed: Literal["y"]
    reveal_type(False or "z")  # revealed: Literal["z"]
    reveal_type(False or True)  # revealed: Literal[True]
    reveal_type(False or False)  # revealed: Literal[False]
    reveal_type(foo or False)  # revealed: str & ~AlwaysFalsy | Literal[False]
    reveal_type(foo or True)  # revealed: str & ~AlwaysFalsy | Literal[True]
```

## AND

```py
def _(foo: str):
    reveal_type(True and False)  # revealed: Literal[False]
    reveal_type(False and True)  # revealed: Literal[False]
    reveal_type(foo and False)  # revealed: str & ~AlwaysTruthy | Literal[False]
    reveal_type(foo and True)  # revealed: str & ~AlwaysTruthy | Literal[True]
    reveal_type("x" and "y" and "z")  # revealed: Literal["z"]
    reveal_type("x" and "y" and "")  # revealed: Literal[""]
    reveal_type("" and "y")  # revealed: Literal[""]
```

## Simple function calls to bool

```py
def _(flag: bool):
    if flag:
        x = True
    else:
        x = False

    reveal_type(x)  # revealed: bool
```

## Complex

```py
reveal_type("x" and "y" or "z")  # revealed: Literal["y"]
reveal_type("x" or "y" and "z")  # revealed: Literal["x"]
reveal_type("" and "y" or "z")  # revealed: Literal["z"]
reveal_type("" or "y" and "z")  # revealed: Literal["z"]
reveal_type("x" and "y" or "")  # revealed: Literal["y"]
reveal_type("x" or "y" and "")  # revealed: Literal["x"]
```

## `bool()` function

## Evaluates to builtin

`a.py`:

```py
redefined_builtin_bool: type[bool] = bool

def my_bool(x) -> bool:
    return True
```

```py
from a import redefined_builtin_bool, my_bool

reveal_type(redefined_builtin_bool(0))  # revealed: Literal[False]
reveal_type(my_bool(0))  # revealed: bool
```

## Truthy values

```py
reveal_type(bool(1))  # revealed: Literal[True]
reveal_type(bool((0,)))  # revealed: Literal[True]
reveal_type(bool("NON EMPTY"))  # revealed: Literal[True]
reveal_type(bool(True))  # revealed: Literal[True]

def foo(): ...

reveal_type(bool(foo))  # revealed: Literal[True]
```

## Falsy values

```py
reveal_type(bool(0))  # revealed: Literal[False]
reveal_type(bool(()))  # revealed: Literal[False]
reveal_type(bool(None))  # revealed: Literal[False]
reveal_type(bool(""))  # revealed: Literal[False]
reveal_type(bool(False))  # revealed: Literal[False]
reveal_type(bool())  # revealed: Literal[False]
```

## Ambiguous values

```py
reveal_type(bool([]))  # revealed: bool
reveal_type(bool({}))  # revealed: bool
reveal_type(bool(set()))  # revealed: bool
```

## `__bool__` returning `NoReturn`

```py
from typing import NoReturn

class NotBoolable:
    def __bool__(self) -> NoReturn:
        raise NotImplementedError("This object can't be converted to a boolean")

# TODO: This should emit an error that `NotBoolable` can't be converted to a bool but it currently doesn't
#   because `Never` is assignable to `bool`. This probably requires dead code analysis to fix.
if NotBoolable():
    ...
```

## Not callable `__bool__`

```py
class NotBoolable:
    __bool__: None = None

# error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `NotBoolable`; its `__bool__` method isn't callable"
if NotBoolable():
    ...
```

## Not-boolable union

```py
def test(cond: bool):
    class NotBoolable:
        __bool__: int | None = None if cond else 3

    # error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `NotBoolable`; its `__bool__` method isn't callable"
    if NotBoolable():
        ...
```

## Union with some variants implementing `__bool__` incorrectly

```py
def test(cond: bool):
    class NotBoolable:
        __bool__: None = None

    a = 10 if cond else NotBoolable()

    # error: [unsupported-bool-conversion] "Boolean conversion is unsupported for type `Literal[10] | NotBoolable`; its `__bool__` method isn't callable"
    if a:
        ...
```
