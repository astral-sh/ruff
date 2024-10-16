# Expressions

## OR

```py
def foo() -> str:
    pass

a = True or False
b = 'x' or 'y' or 'z'
c = '' or 'y' or 'z'
d = False or 'z'
e = False or True
f = False or False
g = foo() or False
h = foo() or True

reveal_type(a)  # revealed: Literal[True]
reveal_type(b)  # revealed: Literal["x"]
reveal_type(c)  # revealed: Literal["y"]
reveal_type(d)  # revealed: Literal["z"]
reveal_type(e)  # revealed: Literal[True]
reveal_type(f)  # revealed: Literal[False]
reveal_type(g)  # revealed: str | Literal[False]
reveal_type(h)  # revealed: str | Literal[True]
```

## AND

```py
def foo() -> str:
    pass

a = True and False
b = False and True
c = foo() and False
d = foo() and True
e = 'x' and 'y' and 'z'
f = 'x' and 'y' and ''
g = '' and 'y'

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[False]
reveal_type(c)  # revealed: str | Literal[False]
reveal_type(d)  # revealed: str | Literal[True]
reveal_type(e)  # revealed: Literal["z"]
reveal_type(f)  # revealed: Literal[""]
reveal_type(g)  # revealed: Literal[""]
```

## Simple function calls to bool

```py
def returns_bool() -> bool:
    return True

if returns_bool():
    x = True
else:
    x = False

reveal_type(x)  # revealed: bool
```

## Complex

```py
def foo() -> str:
    pass

a = "x" and "y" or "z"
b = "x" or "y" and "z"
c = "" and "y" or "z"
d = "" or "y" and "z"
e = "x" and "y" or ""
f = "x" or "y" and ""

reveal_type(a)  # revealed: Literal["y"]
reveal_type(b)  # revealed: Literal["x"]
reveal_type(c)  # revealed: Literal["z"]
reveal_type(d)  # revealed: Literal["z"]
reveal_type(e)  # revealed: Literal["y"]
reveal_type(f)  # revealed: Literal["x"]
```

## `bool()` function

## Evaluates to builtin

```py path=a.py
redefined_builtin_bool = bool

def my_bool(x)-> bool: pass
```

```py
from a import redefined_builtin_bool, my_bool
a = redefined_builtin_bool(0)
b = my_bool(0)

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: bool
```

## Truthy values

```py
a = bool(1)
b = bool((0,))
c = bool("NON EMPTY")
d = bool(True)

def foo(): pass
e = bool(foo)

reveal_type(a)  # revealed: Literal[True]
reveal_type(b)  # revealed: Literal[True]
reveal_type(c)  # revealed: Literal[True]
reveal_type(d)  # revealed: Literal[True]
reveal_type(e)  # revealed: Literal[True]
```

## Falsy values

```py
a = bool(0)
b = bool(())
c = bool(None)
d = bool("")
e = bool(False)
f = bool()

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[False]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[False]
reveal_type(e)  # revealed: Literal[False]
reveal_type(f)  # revealed: Literal[False]
```

## Ambiguous values

```py
a = bool([])
b = bool({})
c = bool(set())

reveal_type(a)  # revealed: bool
reveal_type(b)  # revealed: bool
reveal_type(c)  # revealed: bool
```
