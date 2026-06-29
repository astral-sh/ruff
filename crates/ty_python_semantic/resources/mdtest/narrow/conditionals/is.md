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

## `is not` with a constrained `TypeVar`

An `is not` check excludes the object selected by a constrained `TypeVar`. A subsequent identity
check against the same value is therefore unreachable.

```py
from types import EllipsisType
from typing import TypeVar
from typing_extensions import assert_never

T = TypeVar("T", None, EllipsisType)

def f(value: int | None | EllipsisType, other: T) -> None:
    if value is not other:
        if value is other:
            assert_never(value)
```

## Tuple narrowing with a constrained `TypeVar`

The `is` check below can discard `tuple[int]` because its element cannot be `None` or `...`. The
`is not` check cannot discard either remaining tuple: depending on the current constraint, either
one could differ from `other`.

```py
from types import EllipsisType
from typing import TypeVar

T = TypeVar("T", None, EllipsisType)

def takes_singleton_tuple(value: tuple[None] | tuple[EllipsisType]) -> None: ...
def f(value: tuple[int] | tuple[None] | tuple[EllipsisType], other: T) -> None:
    if value[0] is other:
        takes_singleton_tuple(value)
    if value[0] is not other:
        reveal_type(value)  # revealed: tuple[int] | tuple[None] | tuple[EllipsisType]
```

## `is` with `NewType`s

### Distinct `NewType`s with the same base

Calling a `NewType` returns its argument unchanged. Values with distinct `NewType`s over `Foo` can
therefore be the same object even though their types are distinct. The examples below cover direct
comparisons and narrowing through unions, intersections, and tuple elements.

```py
from typing import NewType
from ty_extensions import Intersection

class Foo: ...
class FooSub(Foo): ...

FooNewType1 = NewType("FooNewType1", Foo)
FooNewType2 = NewType("FooNewType2", Foo)
IntNewType1 = NewType("IntNewType1", int)
IntNewType2 = NewType("IntNewType2", int)

def same_base(foo1: FooNewType1, foo2: FooNewType2) -> None:
    reveal_type(foo1 is foo2)  # revealed: bool
    if foo1 is foo2:
        reveal_type(foo1)  # revealed: FooNewType1
        reveal_type(foo2)  # revealed: FooNewType2

def union(value: FooNewType1 | None, other: FooNewType2) -> None:
    if value is other:
        reveal_type(value)  # revealed: FooNewType1

def intersection(left: Intersection[FooNewType1, FooSub], right: FooNewType2) -> None:
    if left is right:
        reveal_type(right)  # revealed: FooNewType2 & FooSub

def tuple_element(value: tuple[IntNewType1] | tuple[str], other: IntNewType2) -> None:
    if value[0] is other:
        reveal_type(value)  # revealed: tuple[IntNewType1]
```

### `NewType`s in `TypeVar` bounds and constraints

`NewType`s inside `TypeVar` bounds and constraints can likewise refer to the same runtime object.
Comparing the distinct `TypeVar`s below is not always false, and a true branch keeps the original
`TypeVar`.

```py
from typing import NewType, TypeVar

class Foo: ...

FooNewType1 = NewType("FooNewType1", Foo)
FooNewType2 = NewType("FooNewType2", Foo)
FooNewType3 = NewType("FooNewType3", Foo)
FooNewType4 = NewType("FooNewType4", Foo)

BoundedT = TypeVar("BoundedT", bound=FooNewType1)
BoundedU = TypeVar("BoundedU", bound=FooNewType2)

def bounded_typevars(left: BoundedT, right: BoundedU) -> None:
    reveal_type(left is right)  # revealed: bool
    if left is right:
        reveal_type(left)  # revealed: BoundedT@bounded_typevars

ConstrainedT = TypeVar("ConstrainedT", FooNewType1, FooNewType2)
ConstrainedU = TypeVar("ConstrainedU", FooNewType3, FooNewType4)

def constrained_typevars(left: ConstrainedT, right: ConstrainedU) -> None:
    reveal_type(left is right)  # revealed: bool
    if left is right:
        reveal_type(left)  # revealed: ConstrainedT@constrained_typevars
```

Every constraint below is a `NewType` based on `EllipsisType`, so `other` always refers to the same
`...` object as a `SingletonC` value. After an `is not` check, repeating the opposite check must be
unreachable, both directly and when narrowing a tuple.

```py
from types import EllipsisType
from typing import NewType, TypeVar
from typing_extensions import assert_never

SingletonA = NewType("SingletonA", EllipsisType)
SingletonB = NewType("SingletonB", EllipsisType)
SingletonC = NewType("SingletonC", EllipsisType)

SingletonT = TypeVar("SingletonT", SingletonA, SingletonB)

def direct(value: SingletonC | int, other: SingletonT) -> None:
    if value is not other:
        if value is other:
            assert_never(value)

def tuple_element(value: tuple[SingletonC] | tuple[int], other: SingletonT) -> None:
    if value[0] is not other:
        if value[0] is other:
            assert_never(value)
```

### Preserving a `NewType` in the true branch

If an object is identical to a value with a `NewType`, the true branch preserves the `NewType`
rather than replacing it with its underlying type. The call below should not emit an error.

```py
from typing import NewType

UserId = NewType("UserId", int)

def takes_user_id(value: UserId) -> None: ...
def preserve_newtype(x: object, user_id: UserId) -> None:
    if x is user_id:
        takes_user_id(x)
```

### Comparing `NewType`s with literals

Calls to `NewType` return their arguments unchanged. Comparisons with `bool` and `int` literals can
therefore succeed, so the true branches below remain reachable.

```py
from typing import NewType

BoolNewType = NewType("BoolNewType", bool)
IntNewType = NewType("IntNewType", int)

def literals() -> None:
    true = True
    forty_two = 42

    if BoolNewType(true) is true:
        reveal_type(true)  # revealed: Literal[True]
    if IntNewType(forty_two) is forty_two:
        reveal_type(forty_two)  # revealed: Literal[42]
```

### `is not` with singleton `NewType`s

Both `NewType`s below are based on `EllipsisType`, which contains only the `...` object. The
`is not` branch therefore removes the `NewType` alternative. The same rule applies when the check
narrows a tuple.

```py
from types import EllipsisType
from typing import NewType

SingletonA = NewType("SingletonA", EllipsisType)
SingletonB = NewType("SingletonB", EllipsisType)

def singleton_is_not(value: SingletonA | int, other: SingletonB) -> None:
    if value is not other:
        reveal_type(value)  # revealed: int

def tuple_is_not(value: tuple[SingletonA] | tuple[int], other: SingletonB) -> None:
    if value[0] is not other:
        reveal_type(value)  # revealed: tuple[int]
```

### Static exclusions

The type `Not[Literal[True]]` excludes the literal type but accepts the distinct `BoolNewType`.
However, `BoolNewType(True)` returns `True` unchanged, so `value is True` can be either true or
false.

```py
from typing import Literal, NewType
from ty_extensions import Not

BoolNewType = NewType("BoolNewType", bool)

def excludes_true(value: Not[Literal[True]]) -> None:
    reveal_type(value is True)  # revealed: bool

excludes_true(BoolNewType(True))
```

Similarly, `Intersection[int, Not[Literal[1]]]` accepts `IntNewType(1)`, which returns the `1`
object unchanged, so the comparison remains possible.

```py
from typing import Literal, NewType
from ty_extensions import Intersection, Not

IntNewType = NewType("IntNewType", int)

def excludes_one(value: Intersection[int, Not[Literal[1]]]) -> None:
    reveal_type(value is 1)  # revealed: bool

excludes_one(IntNewType(1))
```

### Comparisons that are always false

An identity comparison is still always false when the two runtime types are distinct final classes.

```py
from typing import NewType, final

@final
class A: ...

@final
class B: ...

ANewType = NewType("ANewType", A)
BNewType = NewType("BNewType", B)

def disjoint_bases(a: ANewType, b: BNewType) -> None:
    reveal_type(a is b)  # revealed: Literal[False]
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
