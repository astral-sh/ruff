# Boolean

## Literals

### Simple

```py
x = True
y = False
reveal_type(x)  # revealed: Literal[True]
reveal_type(y)  # revealed: Literal[False]
```

### None

```py
a = not None
b = not not None
reveal_type(a)  # revealed: Literal[True]
reveal_type(b)  # revealed: Literal[False]
```

### TODO: Not function

Unknown should not be part of the type of typing.reveal_type

```py
from typing import reveal_type

def f():
    return 1

a = not f
b = not reveal_type

reveal_type(a)  # revealed: Literal[False]
# reveal_type(b)  # TODO: revealed: Literal[False]
```

### Not module

```py
import b; import warnings

x = not b
z = not warnings

reveal_type(x)  # revealed: Literal[False]
reveal_type(z)  # revealed: Literal[False]
```

```py path=b.py
y = 1
```

### Not union

```py
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

a = not p
b = not q
c = not r
d = not s
e = not t

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: bool
reveal_type(c)  # revealed: bool
reveal_type(d)  # revealed: bool
reveal_type(e)  # revealed: Literal[True]
```

### Not integer literal

```py
a = not 1
b = not 1234567890987654321
e = not 0
x = not -1
y = not -1234567890987654321
z = not --987

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[False]
reveal_type(e)  # revealed: Literal[True]
reveal_type(x)  # revealed: Literal[False]
reveal_type(y)  # revealed: Literal[False]
reveal_type(z)  # revealed: Literal[False]
```

### Not boolean literal

```py
w = True
x = False
y = not w
z = not x

reveal_type(w)  # revealed: Literal[True]
reveal_type(x)  # revealed: Literal[False]
reveal_type(y)  # revealed: Literal[False]
reveal_type(z)  # revealed: Literal[True]
```

### Not string literal

```py
a = not "hello"
b = not ""
c = not "0"
d = not "hello" + "world"

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[True]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[False]
```

### Not bytes literal

```py
a = not b"hello"
b = not b""
c = not b"0"
d = not b"hello" + b"world"

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[True]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[False]
```

### Not tuple

```py
a = not (1,)
b = not (1, 2)
c = not (1, 2, 3)
d = not ()
e = not ("hello",)
f = not (1, "hello")

reveal_type(a)  # revealed: Literal[False]
reveal_type(b)  # revealed: Literal[False]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[True]
reveal_type(e)  # revealed: Literal[False]
reveal_type(f)  # revealed: Literal[False]
```

## Comparison

### Integer literals

```py
a = 1 == 1 == True
b = 1 == 1 == 2 == 4
c = False < True <= 2 < 3 != 6
d = 1 < 1
e = 1 > 1
f = 1 is 1
g = 1 is not 1
h = 1 is 2
i = 1 is not 7
j = 1 <= "" and 0 < 1

reveal_type(a)  # revealed: Literal[True]
reveal_type(b)  # revealed: Literal[False]
reveal_type(c)  # revealed: Literal[True]
reveal_type(d)  # revealed: Literal[False]
reveal_type(e)  # revealed: Literal[False]
reveal_type(f)  # revealed: bool
reveal_type(g)  # revealed: bool
reveal_type(h)  # revealed: Literal[False]
reveal_type(i)  # revealed: Literal[True]
reveal_type(j)  # revealed: @Todo | Literal[True]
```

### Integer instance

TODO: implement lookup of `__eq__` on typeshed `int` stub.

```py
def int_instance() -> int: ...
a = 1 == int_instance()
b = 9 < int_instance()
c = int_instance() < int_instance()

reveal_type(a)  # revealed: @Todo
reveal_type(b)  # revealed: bool
reveal_type(c)  # revealed: bool
```

### Non boolean returns

Walking through examples:

- `a = A() < B() < C()`

    1. `A() < B() and B() < C()` - split in N comparison
    1. `A()` and `B()`              - evaluate outcome types
    1. `bool` and `bool`            - evaluate truthiness
    1. `A | B`                    - union of "first true" types

- `b = 0 < 1 < A() < 3`

    1. `0 < 1 and 1 < A() and A() < 3` - split in N comparison
    1. `True` and `bool` and `A` - evaluate outcome types
    1. `True` and `bool` and `bool` - evaluate truthiness
    1. `bool | A` - union of "true" types

- `c = 10 < 0 < A() < B() < C()` short-cicuit to False

```py
from __future__ import annotations
class A:
    def __lt__(self, other) -> A: ...
class B:
    def __lt__(self, other) -> B: ...
class C:
    def __lt__(self, other) -> C: ...

a = A() < B() < C()
b = 0 < 1 < A() < 3
c = 10 < 0 < A() < B() < C()

reveal_type(a)  # revealed: A | B
reveal_type(b)  # revealed: bool | A
reveal_type(c)  # revealed: Literal[False]
```

### String literals

NOTE: `j = "ab" < "ab_cd"` is a very cornercase test ensuring we're not comparing the interned salsa symbols, which compare by order of declaration.

```py
def str_instance() -> str: ...
a = "abc" == "abc"
b = "ab_cd" <= "ab_ce"
c = "abc" in "ab cd"
d = "" not in "hello"
e = "--" is "--"
f = "A" is "B"
g = "--" is not "--"
h = "A" is not "B"
i = str_instance() < "..."
j = "ab" < "ab_cd"

reveal_type(a)  # revealed: Literal[True]
reveal_type(b)  # revealed: Literal[True]
reveal_type(c)  # revealed: Literal[False]
reveal_type(d)  # revealed: Literal[False]
reveal_type(e)  # revealed: bool
reveal_type(f)  # revealed: Literal[False]
reveal_type(g)  # revealed: bool
reveal_type(h)  # revealed: Literal[True]
reveal_type(i)  # revealed: bool
reveal_type(j)  # revealed: Literal[True]
```

### Unsupported operators

TODO: `d = 5 < object()` should be `Unknown` but we don't check if __lt__ signature is valid for right operand type.

```py
a = 1 in 7      # error: "Operator `in` is not supported for types `Literal[1]` and `Literal[7]`"
b = 0 not in 10 # error: "Operator `not in` is not supported for types `Literal[0]` and `Literal[10]`"
c = object() < 5 # error: "Operator `<` is not supported for types `object` and `Literal[5]`"
d = 5 < object()

reveal_type(a)  # revealed: bool
reveal_type(b)  # revealed: bool
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: bool
```

## Expressions

### OR

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

### AND

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

### Simple function calls to bool

```py
def returns_bool() -> bool:
    return True

if returns_bool():
    x = True
else:
    x = False

reveal_type(x)  # revealed: bool
```

### Complex

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

### Evaluates to builtin

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

### Truthy values

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

### Falsy values

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

### Ambiguous values

```py
a = bool([])
b = bool({})
c = bool(set())

reveal_type(a)  # revealed: bool
reveal_type(b)  # revealed: bool
reveal_type(c)  # revealed: bool
```
