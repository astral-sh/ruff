# Narrowing for `issubclass` checks

Narrowing for `issubclass(class, classinfo)` expressions.

## `classinfo` is a single type

### Basic example

```py
def _(flag: bool):
    t = int if flag else str

    if issubclass(t, bytes):
        reveal_type(t)  # revealed: Never

    if issubclass(t, object):
        reveal_type(t)  # revealed: Literal[int, str]

    if issubclass(t, int):
        reveal_type(t)  # revealed: Literal[int]
    else:
        reveal_type(t)  # revealed: Literal[str]

    if issubclass(t, str):
        reveal_type(t)  # revealed: Literal[str]
        if issubclass(t, int):
            reveal_type(t)  # revealed: Never
```

### Proper narrowing in `elif` and `else` branches

```py
def _(flag1: bool, flag2: bool):
    t = int if flag1 else str if flag2 else bytes

    if issubclass(t, int):
        reveal_type(t)  # revealed: Literal[int]
    else:
        reveal_type(t)  # revealed: Literal[str, bytes]

    if issubclass(t, int):
        reveal_type(t)  # revealed: Literal[int]
    elif issubclass(t, str):
        reveal_type(t)  # revealed: Literal[str]
    else:
        reveal_type(t)  # revealed: Literal[bytes]
```

### Multiple derived classes

```py
class Base: ...
class Derived1(Base): ...
class Derived2(Base): ...
class Unrelated: ...

def _(flag1: bool, flag2: bool, flag3: bool):
    t1 = Derived1 if flag1 else Derived2

    if issubclass(t1, Base):
        reveal_type(t1)  # revealed: Literal[Derived1, Derived2]

    if issubclass(t1, Derived1):
        reveal_type(t1)  # revealed: Literal[Derived1]
    else:
        reveal_type(t1)  # revealed: Literal[Derived2]

    t2 = Derived1 if flag2 else Base

    if issubclass(t2, Base):
        reveal_type(t2)  # revealed: Literal[Derived1, Base]

    t3 = Derived1 if flag3 else Unrelated

    if issubclass(t3, Base):
        reveal_type(t3)  # revealed: Literal[Derived1]
    else:
        reveal_type(t3)  # revealed: Literal[Unrelated]
```

### Narrowing for non-literals

```py
class A: ...
class B: ...

def _(t: type[object]):
    if issubclass(t, A):
        reveal_type(t)  # revealed: type[A]
        if issubclass(t, B):
            reveal_type(t)  # revealed: type[A] & type[B]
    else:
        reveal_type(t)  # revealed: type & ~type[A]
```

### Handling of `None`

`types.NoneType` is only available in Python 3.10 and later:

```toml
[environment]
python-version = "3.10"
```

```py
from types import NoneType

def _(flag: bool):
    t = int if flag else NoneType

    if issubclass(t, NoneType):
        reveal_type(t)  # revealed: Literal[NoneType]

    if issubclass(t, type(None)):
        # TODO: this should be just `Literal[NoneType]`
        reveal_type(t)  # revealed: Literal[int, NoneType]
```

## `classinfo` contains multiple types

### (Nested) tuples of types

```py
class Unrelated: ...

def _(flag1: bool, flag2: bool):
    t = int if flag1 else str if flag2 else bytes

    if issubclass(t, (int, (Unrelated, (bytes,)))):
        reveal_type(t)  # revealed: Literal[int, bytes]
    else:
        reveal_type(t)  # revealed: Literal[str]
```

## Special cases

### Emit a diagnostic if the first argument is of wrong type

#### Too wide

`type[object]` is a subtype of `object`, but not every `object` can be passed as the first argument
to `issubclass`:

```py
class A: ...

t = object()

# error: [invalid-argument-type]
if issubclass(t, A):
    reveal_type(t)  # revealed: type[A]
```

#### Wrong

`Literal[1]` and `type` are entirely disjoint, so the inferred type of `Literal[1] & type[int]` is
eagerly simplified to `Never` as a result of the type narrowing in the `if issubclass(t, int)`
branch:

```py
t = 1

# error: [invalid-argument-type]
if issubclass(t, int):
    reveal_type(t)  # revealed: Never
```

### Do not use custom `issubclass` for narrowing

```py
def issubclass(c, ci):
    return True

def flag() -> bool: ...

t = int if flag() else str
if issubclass(t, int):
    reveal_type(t)  # revealed: Literal[int, str]
```

### Do support narrowing if `issubclass` is aliased

```py
issubclass_alias = issubclass

def flag() -> bool: ...

t = int if flag() else str
if issubclass_alias(t, int):
    reveal_type(t)  # revealed: Literal[int]
```

### Do support narrowing if `issubclass` is imported

```py
from builtins import issubclass as imported_issubclass

def flag() -> bool: ...

t = int if flag() else str
if imported_issubclass(t, int):
    reveal_type(t)  # revealed: Literal[int]
```

### Do not narrow if second argument is not a proper `classinfo` argument

```py
from typing import Any

def flag() -> bool: ...

t = int if flag() else str

# TODO: this should cause us to emit a diagnostic during
# type checking
if issubclass(t, "str"):
    reveal_type(t)  # revealed: Literal[int, str]

# TODO: this should cause us to emit a diagnostic during
# type checking
if issubclass(t, (bytes, "str")):
    reveal_type(t)  # revealed: Literal[int, str]

# TODO: this should cause us to emit a diagnostic during
# type checking
if issubclass(t, Any):
    reveal_type(t)  # revealed: Literal[int, str]
```

### Do not narrow if there are keyword arguments

```py
def flag() -> bool: ...

t = int if flag() else str

# error: [unknown-argument]
if issubclass(t, int, foo="bar"):
    reveal_type(t)  # revealed: Literal[int, str]
```

### `type[]` types are narrowed as well as class-literal types

```py
def _(x: type, y: type[int]):
    if issubclass(x, y):
        reveal_type(x)  # revealed: type[int]
```
