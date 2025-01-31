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

```py path=a.py
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
