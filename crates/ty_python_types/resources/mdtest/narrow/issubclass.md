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
        reveal_type(t)  # revealed: <class 'int'> | <class 'str'>

    if issubclass(t, int):
        reveal_type(t)  # revealed: <class 'int'>
    else:
        reveal_type(t)  # revealed: <class 'str'>

    if issubclass(t, str):
        reveal_type(t)  # revealed: <class 'str'>
        if issubclass(t, int):
            reveal_type(t)  # revealed: Never
```

### Proper narrowing in `elif` and `else` branches

```py
def _(flag1: bool, flag2: bool):
    t = int if flag1 else str if flag2 else bytes

    if issubclass(t, int):
        reveal_type(t)  # revealed: <class 'int'>
    else:
        reveal_type(t)  # revealed: <class 'str'> | <class 'bytes'>

    if issubclass(t, int):
        reveal_type(t)  # revealed: <class 'int'>
    elif issubclass(t, str):
        reveal_type(t)  # revealed: <class 'str'>
    else:
        reveal_type(t)  # revealed: <class 'bytes'>
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
        reveal_type(t1)  # revealed: <class 'Derived1'> | <class 'Derived2'>

    if issubclass(t1, Derived1):
        reveal_type(t1)  # revealed: <class 'Derived1'>
    else:
        reveal_type(t1)  # revealed: <class 'Derived2'>

    t2 = Derived1 if flag2 else Base

    if issubclass(t2, Base):
        reveal_type(t2)  # revealed: <class 'Derived1'> | <class 'Base'>

    t3 = Derived1 if flag3 else Unrelated

    if issubclass(t3, Base):
        reveal_type(t3)  # revealed: <class 'Derived1'>
    else:
        reveal_type(t3)  # revealed: <class 'Unrelated'>
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
        reveal_type(t)  # revealed: <class 'NoneType'>

    if issubclass(t, type(None)):
        reveal_type(t)  # revealed: <class 'NoneType'>
```

## `classinfo` contains multiple types

### (Nested) tuples of types

```py
class Unrelated: ...

def _(flag1: bool, flag2: bool):
    t = int if flag1 else str if flag2 else bytes

    if issubclass(t, (int, (Unrelated, (bytes,)))):
        reveal_type(t)  # revealed: <class 'int'> | <class 'bytes'>
    else:
        reveal_type(t)  # revealed: <class 'str'>
```

## `classinfo` is a PEP-604 union of types

```toml
[environment]
python-version = "3.10"
```

```py
def f(x: type[int | str | bytes | range]):
    if issubclass(x, int | str):
        reveal_type(x)  # revealed: type[int] | type[str]
    elif issubclass(x, bytes | memoryview):
        reveal_type(x)  # revealed: type[bytes]
    else:
        reveal_type(x)  # revealed: <class 'range'>
```

Although `issubclass()` usually only works if all elements in the `UnionType` are class objects, at
runtime a special exception is made for `None` so that `issubclass(x, int | None)` can work:

```py
def _(x: type):
    if issubclass(x, int | str | None):
        reveal_type(x)  # revealed: type[int] | type[str] | <class 'NoneType'>
    else:
        reveal_type(x)  # revealed: type & ~type[int] & ~type[str] & ~<class 'NoneType'>
```

## `classinfo` is an invalid PEP-604 union of types

Except for the `None` special case mentioned above, narrowing can only take place if all elements in
the PEP-604 union are class literals. If any elements are generic aliases or other types, the
`issubclass()` call may fail at runtime, so no narrowing can take place:

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.10"
```

```py
def _(x: type[int | list | bytes]):
    # error: [invalid-argument-type]
    if issubclass(x, int | list[int]):
        reveal_type(x)  # revealed: type[int] | type[list[Unknown]] | type[bytes]
    else:
        reveal_type(x)  # revealed: type[int] | type[list[Unknown]] | type[bytes]
```

## PEP-604 unions on Python \<3.10

PEP-604 unions were added in Python 3.10, so attempting to use them on Python 3.9 does not lead to
any type narrowing.

```toml
[environment]
python-version = "3.9"
```

```py
def _(x: type[int | str | bytes]):
    # error: [unsupported-operator]
    if issubclass(x, int | str):
        reveal_type(x)  # revealed: (type[int] & Unknown) | (type[str] & Unknown) | (type[bytes] & Unknown)
    else:
        reveal_type(x)  # revealed: (type[int] & Unknown) | (type[str] & Unknown) | (type[bytes] & Unknown)
```

## `classinfo` is a `types.UnionType`

Python 3.10 added the ability to use `Union[int, str]` as the second argument to `issubclass()`:

```py
from typing import Union

IntOrStr = Union[int, str]

reveal_type(IntOrStr)  # revealed: <types.UnionType special-form 'int | str'>

def f(x: type[int | str | bytes | range]):
    if issubclass(x, IntOrStr):
        reveal_type(x)  # revealed: type[int] | type[str]
    elif issubclass(x, Union[bytes, memoryview]):
        reveal_type(x)  # revealed: type[bytes]
    else:
        reveal_type(x)  # revealed: <class 'range'>
```

## `classinfo` is a generic final class

```toml
[environment]
python-version = "3.12"
```

When we check a generic `@final` class against `type[GenericFinal]`, we can conclude that the check
always succeeds:

```py
from typing import final

@final
class GenericFinal[T]:
    x: T  # invariant

def f(x: type[GenericFinal]):
    reveal_type(x)  # revealed: <class 'GenericFinal[Unknown]'>

    if issubclass(x, GenericFinal):
        reveal_type(x)  # revealed: <class 'GenericFinal[Unknown]'>
    else:
        reveal_type(x)  # revealed: Never
```

This also works if the typevar has an upper bound:

```py
@final
class BoundedGenericFinal[T: int]:
    x: T  # invariant

def g(x: type[BoundedGenericFinal]):
    reveal_type(x)  # revealed: <class 'BoundedGenericFinal[Unknown]'>

    if issubclass(x, BoundedGenericFinal):
        reveal_type(x)  # revealed: <class 'BoundedGenericFinal[Unknown]'>
    else:
        reveal_type(x)  # revealed: Never
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

def flag() -> bool:
    return True

t = int if flag() else str
if issubclass(t, int):
    reveal_type(t)  # revealed: <class 'int'> | <class 'str'>
```

### Do support narrowing if `issubclass` is aliased

```py
issubclass_alias = issubclass

def flag() -> bool:
    return True

t = int if flag() else str
if issubclass_alias(t, int):
    reveal_type(t)  # revealed: <class 'int'>
```

### Do support narrowing if `issubclass` is imported

```py
from builtins import issubclass as imported_issubclass

def flag() -> bool:
    return True

t = int if flag() else str
if imported_issubclass(t, int):
    reveal_type(t)  # revealed: <class 'int'>
```

### Do not narrow if second argument is not a proper `classinfo` argument

```py
from typing import Any

def flag() -> bool:
    return True

t = int if flag() else str

# error: [invalid-argument-type] "Argument to function `issubclass` is incorrect: Expected `type | UnionType | tuple[Divergent, ...]`, found `Literal["str"]"
if issubclass(t, "str"):
    reveal_type(t)  # revealed: <class 'int'> | <class 'str'>

# TODO: this should cause us to emit a diagnostic during
# type checking
if issubclass(t, (bytes, "str")):
    reveal_type(t)  # revealed: <class 'int'> | <class 'str'>

# TODO: this should cause us to emit a diagnostic during
# type checking
if issubclass(t, Any):
    reveal_type(t)  # revealed: <class 'int'> | <class 'str'>
```

### Do not narrow if there are keyword arguments

```py
def flag() -> bool:
    return True

t = int if flag() else str

# error: [unknown-argument]
if issubclass(t, int, foo="bar"):
    reveal_type(t)  # revealed: <class 'int'> | <class 'str'>
```

### `type[]` types are narrowed as well as class-literal types

```py
def _(x: type, y: type[int]):
    if issubclass(x, y):
        reveal_type(x)  # revealed: type[int]
```

### Disjoint `type[]` types are narrowed to `Never`

Here, `type[UsesMeta1]` and `type[UsesMeta2]` are disjoint because a common subclass of `UsesMeta1`
and `UsesMeta2` could only exist if a common subclass of their metaclasses could exist. This is
known to be impossible due to the fact that `Meta1` is marked as `@final`.

```py
from typing import final

@final
class Meta1(type): ...

class Meta2(type): ...
class UsesMeta1(metaclass=Meta1): ...
class UsesMeta2(metaclass=Meta2): ...

def _(x: type[UsesMeta1], y: type[UsesMeta2]):
    if issubclass(x, y):
        reveal_type(x)  # revealed: Never
    else:
        reveal_type(x)  # revealed: type[UsesMeta1]

    if issubclass(y, x):
        reveal_type(y)  # revealed: Never
    else:
        reveal_type(y)  # revealed: type[UsesMeta2]
```

## Narrowing if an object with an intersection/union/TypeVar type is used as the second argument

If an intersection with only positive members is used as the second argument, and all positive
members of the intersection are valid arguments for the second argument to `isinstance()`, we
intersect with each positive member of the intersection:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, ClassVar
from ty_extensions import Intersection

class Foo: ...

class Bar:
    attribute: ClassVar[int]

class Baz:
    attribute: ClassVar[str]

def f(x: type[Foo], y: Intersection[type[Bar], type[Baz]], z: type[Any]):
    if issubclass(x, y):
        reveal_type(x)  # revealed: type[Foo] & type[Bar] & type[Baz]

    if issubclass(x, z):
        reveal_type(x)  # revealed: type[Foo] & Any
```

The same if a union type is used:

```py
def g(x: type[Foo], y: type[Bar | Baz]):
    if issubclass(x, y):
        reveal_type(x)  # revealed: (type[Foo] & type[Bar]) | (type[Foo] & type[Baz])
```

And even if a `TypeVar` is used, providing it has valid upper bounds/constraints:

```py
from typing import TypeVar

T = TypeVar("T", bound=type[Bar])

def h_old_syntax(x: type[Foo], y: T) -> T:
    if issubclass(x, y):
        reveal_type(x)  # revealed: type[Foo] & type[Bar]
        reveal_type(x.attribute)  # revealed: int

    return y

def h[U: type[Bar | Baz]](x: type[Foo], y: U) -> U:
    if issubclass(x, y):
        reveal_type(x)  # revealed: (type[Foo] & type[Bar]) | (type[Foo] & type[Baz])
        reveal_type(x.attribute)  # revealed: int | str

    return y
```

Or even a tuple of tuple of typevars that have intersection bounds...

```py
from ty_extensions import Intersection

class Spam: ...
class Eggs: ...
class Ham: ...
class Mushrooms: ...

def i[T: Intersection[type[Bar], type[Baz | Spam]], U: (type[Eggs], type[Ham])](x: type[Foo], y: T, z: U) -> tuple[T, U]:
    if issubclass(x, (y, (z, Mushrooms))):
        # revealed: (type[Foo] & type[Bar] & type[Baz]) | (type[Foo] & type[Bar] & type[Spam]) | (type[Foo] & type[Eggs]) | (type[Foo] & type[Ham]) | (type[Foo] & type[Mushrooms])
        reveal_type(x)

    return (y, z)
```
