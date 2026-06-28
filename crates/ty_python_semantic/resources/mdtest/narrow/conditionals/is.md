# Narrowing for `is` conditionals

## `is None`

```py
from typing import Literal

def _(x: None | Literal[1]):
    if x is None:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: Literal[1]

    reveal_type(x)  # revealed: None | Literal[1]
```

## `is` for other types

```py
class A: ...

def _(x: A, y: A | None):
    if y is x:
        reveal_type(y)  # revealed: A
    else:
        reveal_type(y)  # revealed: A | None

    reveal_type(y)  # revealed: A | None
```

## `is` in chained comparisons

```py
def _(x: bool, y: bool):
    if y is x is False:  # Interpreted as `(y is x) and (x is False)`
        reveal_type(x)  # revealed: Literal[False]
        reveal_type(y)  # revealed: bool
    else:
        # The negation of the clause above is (y is not x) or (x is not False)
        # So we can't narrow the type of x or y here, because each arm of the `or` could be true
        reveal_type(x)  # revealed: bool
        reveal_type(y)  # revealed: bool
```

## `is` in elif clause

```py
from typing import Literal

def _(x: None | Literal[1, True]):
    if x is None:
        reveal_type(x)  # revealed: None
    elif x is True:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[1]
```

## `is` for enums

```py
from enum import Enum
from typing import Literal

class Answer(Enum):
    NO = 0
    YES = 1

def _(answer: Answer):
    if answer is Answer.NO:
        reveal_type(answer)  # revealed: Literal[Answer.NO]
    else:
        reveal_type(answer)  # revealed: Literal[Answer.YES]

class Single(Enum):
    VALUE = 1

def _(x: Single | int):
    if x is Single.VALUE:
        reveal_type(x)  # revealed: Single
    else:
        reveal_type(x)  # revealed: int

def _(x: list[int] | Literal[Answer.NO]):
    if x is Answer.NO:
        return
    reveal_type(x)  # revealed: list[int]
```

## `is` for `EllipsisType`

```py
from types import EllipsisType

def _(x: int | EllipsisType):
    if x is ...:
        reveal_type(x)  # revealed: EllipsisType
    else:
        reveal_type(x)  # revealed: int
```

## Assignment expressions

```py
from typing import Literal

def f() -> Literal[1, 2] | None: ...

if (x := f()) is None:
    reveal_type(x)  # revealed: None
else:
    reveal_type(x)  # revealed: Literal[1, 2]

value = f()
if result := (value is None):
    reveal_type(value)  # revealed: None
    reveal_type(result)  # revealed: Literal[True]
else:
    reveal_type(value)  # revealed: Literal[1, 2]
    reveal_type(result)  # revealed: Literal[False]

value = f()
if value := (value is None):
    reveal_type(value)  # revealed: Literal[True]
else:
    reveal_type(value)  # revealed: Literal[False]
```

## `is` with two narrowable operands

Both operands should be narrowed when both are narrowable expressions.

```py
from typing import Literal

def _(t: Literal[True], tn: Literal[True] | None):
    if tn is t:
        reveal_type(tn)  # revealed: Literal[True]
    if t is tn:
        reveal_type(tn)  # revealed: Literal[True]
```

Both operands should also be narrowed in chained comparisons:

```py
from typing import Literal

def _(a: Literal[1], b: Literal[1, 2], c: Literal[1, 2, 3]):
    if a is b is c:
        reveal_type(b)  # revealed: Literal[1]
        reveal_type(c)  # revealed: Literal[1]
```

When a generic class object is compared with an exact class object, the exact class object is not
widened to the generic type. The intersection is retained because it preserves the relationship
between the class object and `T`:

```toml
[environment]
python-version = "3.12"
```

```py
class Y:
    def __init__(self) -> None: ...

class Z(Y):
    def __init__(self, x: int) -> None: ...

def narrow[T: (Y, Z)](klass: type[T]) -> None:
    if klass is Y:
        reveal_type(klass)  # revealed: type[T@narrow] & <class 'Y'>
        reveal_type(Y)  # revealed: <class 'Y'> & type[T@narrow]

    if klass is Z:
        reveal_type(klass)  # revealed: <class 'Z'>
        reveal_type(Z)  # revealed: <class 'Z'>

def construct[T: (Y, Z)](klass: type[T]) -> T:
    if klass is Y:
        return Y()
    raise AssertionError

class Generic[T]: ...
class Specialized(Generic[int]): ...

def narrow_generic_alias[T: (Generic[int], Specialized)](klass: type[T]) -> None:
    if klass is Generic[int]:
        reveal_type(klass)  # revealed: type[T@narrow_generic_alias] & <class 'Generic[int]'>
        reveal_type(Generic[int])  # revealed: <class 'Generic[int]'>
```

## `is` with `NewType`s

Distinct `NewType`s can describe the same runtime object even though they are statically disjoint.
An identity comparison between them must not be considered always false or narrow either operand:

```py
from typing import NewType, final
from ty_extensions import Intersection, is_disjoint_from, static_assert

class Foo: ...
class FooSub(Foo): ...

FooNewType1 = NewType("FooNewType1", Foo)
FooNewType2 = NewType("FooNewType2", Foo)

static_assert(is_disjoint_from(FooNewType1, FooNewType2))

def same_base(foo1: FooNewType1, foo2: FooNewType2) -> None:
    reveal_type(foo1 is foo2)  # revealed: bool
    if foo1 is foo2:
        reveal_type(foo1)  # revealed: FooNewType1
        reveal_type(foo2)  # revealed: FooNewType2

def unions(left: FooNewType1 | str, right: FooNewType2 | bytes) -> None:
    reveal_type(left is right)  # revealed: bool
    if left is right:
        reveal_type(left)  # revealed: FooNewType1 | (str & Foo)
        reveal_type(right)  # revealed: FooNewType2 | (bytes & Foo)

def intersection(left: Intersection[FooNewType1, FooSub], right: FooNewType2) -> None:
    reveal_type(left is right)  # revealed: bool
    if left is right:
        reveal_type(left)  # revealed: FooNewType1 & FooSub
        reveal_type(right)  # revealed: FooNewType2 & FooSub

BoolNewType = NewType("BoolNewType", bool)
IntNewType = NewType("IntNewType", int)
StrNewType = NewType("StrNewType", str)
BytesNewType = NewType("BytesNewType", bytes)

def literals(
    bool_newtype: BoolNewType,
    int_newtype: IntNewType,
    str_newtype: StrNewType,
    bytes_newtype: BytesNewType,
) -> None:
    true = True
    forty_two = 42
    some_string = "some_string"
    some_bytes = b"some_bytes"

    reveal_type(bool_newtype is true)  # revealed: bool
    if bool_newtype is true:
        reveal_type(true)  # revealed: Literal[True]
    if int_newtype is forty_two:
        reveal_type(forty_two)  # revealed: Literal[42]
    if str_newtype is some_string:
        reveal_type(some_string)  # revealed: Literal["some_string"]
    if bytes_newtype is some_bytes:
        reveal_type(some_bytes)  # revealed: Literal[b"some_bytes"]

@final
class A: ...

@final
class B: ...

ANewType = NewType("ANewType", A)
BNewType = NewType("BNewType", B)

def disjoint_bases(a: ANewType, b: BNewType) -> None:
    reveal_type(a is b)  # revealed: Literal[False]
    if a is b:
        reveal_type(a)  # revealed: Never
        reveal_type(b)  # revealed: Never
```

## `is` where the other operand is a call expression

```py
from typing import Literal, final

def foo() -> Literal[42]:
    return 42

def f(x: object):
    if x is foo():
        reveal_type(x)  # revealed: Literal[42]
    else:
        reveal_type(x)  # revealed: object

    if x is not foo():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: Literal[42]

    if foo() is x:
        reveal_type(x)  # revealed: Literal[42]
    else:
        reveal_type(x)  # revealed: object

    if foo() is not x:
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: Literal[42]

def bar() -> int:
    return 42

def g(x: object):
    if x is bar():
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: object

    if x is not bar():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: int

@final
class FinalClass: ...

def baz() -> FinalClass:
    return FinalClass()

def h(x: object):
    if x is baz():
        reveal_type(x)  # revealed: FinalClass
    else:
        reveal_type(x)  # revealed: object

    if x is not baz():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: FinalClass

def spam() -> None:
    return None

def h(x: object):
    if x is spam():
        reveal_type(x)  # revealed: None
    else:
        # `else` narrowing can occur because `spam()` returns a singleton type
        reveal_type(x)  # revealed: ~None

    if x is not spam():
        reveal_type(x)  # revealed: ~None
    else:
        reveal_type(x)  # revealed: None
```
