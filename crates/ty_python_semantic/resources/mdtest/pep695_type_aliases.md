# PEP 695 type aliases

PEP 695 type aliases are only available in Python 3.12 and later:

```toml
[environment]
python-version = "3.12"
```

## Basic

```py
type IntOrStr = int | str

reveal_type(IntOrStr)  # revealed: TypeAliasType
reveal_type(IntOrStr.__name__)  # revealed: Literal["IntOrStr"]

x: IntOrStr = 1

reveal_type(x)  # revealed: Literal[1]

def f() -> None:
    reveal_type(x)  # revealed: int | str
```

## `__value__` attribute

```py
type IntOrStr = int | str

reveal_type(IntOrStr.__value__)  # revealed: Any
```

## Invalid assignment

```py
type OptionalInt = int | None

# error: [invalid-assignment]
x: OptionalInt = "1"
```

## No type qualifiers

The right-hand side of a type alias definition is a type expression, not an annotation expression.
Type qualifiers like `ClassVar` and `Final` are only valid in annotation expressions, so they cannot
appear at the top level of a PEP 695 alias definition:

```py
from typing_extensions import ClassVar, Final, Required, NotRequired, ReadOnly
from dataclasses import InitVar

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in type alias values"
type Bad1 = ClassVar[str]
# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in type alias values"
type Bad2 = ClassVar
# error: [invalid-type-form] "Type qualifier `typing.Final` is not allowed in type alias values"
type Bad3 = Final[int]
# error: [invalid-type-form] "Type qualifier `typing.Final` is not allowed in type alias values"
type Bad4 = Final
# error: [invalid-type-form] "Type qualifier `typing.Required` is not allowed in type alias values"
type Bad5 = Required[int]
# error: [invalid-type-form] "Type qualifier `typing.NotRequired` is not allowed in type alias values"
type Bad6 = NotRequired[int]
# error: [invalid-type-form] "Type qualifier `typing.ReadOnly` is not allowed in type alias values"
type Bad7 = ReadOnly[int]
# error: [invalid-type-form] "Type qualifier `dataclasses.InitVar` is not allowed in type alias values"
type Bad8 = InitVar[int]
# error: [invalid-type-form] "Type qualifier `dataclasses.InitVar` is not allowed in type alias values"
type Bad9 = InitVar
```

## Type aliases in type aliases

```py
type IntOrStr = int | str
type IntOrStrOrBytes = IntOrStr | bytes

x: IntOrStrOrBytes = 1

def f() -> None:
    reveal_type(x)  # revealed: int | str | bytes
```

## Aliased type aliases

```py
type IntOrStr = int | str
MyIntOrStr = IntOrStr

x: MyIntOrStr = 1

# error: [invalid-assignment]
y: MyIntOrStr = None
```

## Unpacking from a type alias

```py
type T = tuple[int, str]

def f(x: T):
    a, b = x
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: str
```

## Scoping

PEP 695 type aliases delay runtime evaluation of their right-hand side, so they are a lazy (not
eager) nested scope.

```py
type Alias = Foo | str

def f(x: Alias):
    reveal_type(x)  # revealed: Foo | str

class Foo:
    pass
```

But narrowing of names used in the type alias is still respected:

```py
def _(flag: bool):
    t = int if flag else None
    if t is not None:
        type Alias = t | str
        def f(x: Alias):
            reveal_type(x)  # revealed: int | str
```

## Generic type aliases

```py
type ListOrSet[T] = list[T] | set[T]
reveal_type(ListOrSet.__type_params__)  # revealed: tuple[TypeVar | ParamSpec | TypeVarTuple, ...]
type Tuple1[T] = tuple[T]

def _(cond: bool):
    Generic = ListOrSet if cond else Tuple1

    def _(x: Generic[int]):
        reveal_type(x)  # revealed: list[int] | set[int] | tuple[int]

try:
    class Foo[T]:
        x: T
        def foo(self) -> T:
            return self.x

    ...
except Exception:
    class Foo[T]:
        x: T
        def foo(self) -> T:
            return self.x

def f(x: Foo[int]):
    reveal_type(x.foo())  # revealed: int
```

## Stringified values

Stringifying the right-hand side of a type alias is redundant, but allowed:

```py
type X = "int | str"

def f(obj: X):
    reveal_type(obj)  # revealed: int | str
```

The right-hand side of a PEP-695 type alias will not usually be executed, but can be if the user
accesses the `.__value__` attribute. Normal runtime rules still therefore apply regarding partially
stringified alias values:

```py
# snapshot: unsupported-operator
type Y = "int" | str

def g(obj: Y):
    reveal_type(obj)  # revealed: int | str
```

```snapshot
error[unsupported-operator]: Unsupported `|` operation
 --> src/mdtest_snippet.py:6:10
  |
6 | type Y = "int" | str
  |          -----^^^---
  |          |       |
  |          |       Has type `<class 'str'>`
  |          Has type `Literal["int"]`
  |
info: A type alias scope is lazy but will be executed at runtime if the `__value__` property is accessed
```

## In unions and intersections

We can "break apart" a type alias by e.g. adding it to a union:

```py
type IntOrStr = int | str

def f(x: IntOrStr, y: str | bytes):
    z = x or y
    reveal_type(z)  # revealed: (int & ~AlwaysFalsy) | str | bytes
```

## Multiple layers of union aliases

```py
class A: ...
class B: ...
class C: ...
class D: ...

type W = A | B
type X = C | D
type Y = W | X

from ty_extensions import is_equivalent_to, static_assert

static_assert(is_equivalent_to(Y, A | B | C | D))
```

## In binary ops

```py
from typing import Literal

type X = tuple[Literal[1], Literal[2]]

def _(x: X, y: tuple[Literal[1], Literal[3]]):
    reveal_type(x == y)  # revealed: Literal[False]
    reveal_type(x < y)  # revealed: Literal[True]
```

## `TypeAliasType` properties

Two `TypeAliasType`s are distinct and disjoint, even if they refer to the same type

```py
from ty_extensions import static_assert, is_equivalent_to, is_disjoint_from, TypeOf

type Alias1 = int
type Alias2 = int

type TypeAliasType1 = TypeOf[Alias1]
type TypeAliasType2 = TypeOf[Alias2]

static_assert(not is_equivalent_to(TypeAliasType1, TypeAliasType2))
static_assert(is_disjoint_from(TypeAliasType1, TypeAliasType2))
```

## Direct use of `TypeAliasType`

`TypeAliasType` can also be used directly. This is useful for versions of Python prior to 3.12.

```toml
[environment]
python-version = "3.9"
```

### Basic example

```py
from typing_extensions import TypeAliasType, Union

IntOrStr = TypeAliasType("IntOrStr", Union[int, str])

reveal_type(IntOrStr)  # revealed: TypeAliasType

reveal_type(IntOrStr.__name__)  # revealed: Literal["IntOrStr"]

def f(x: IntOrStr) -> None:
    reveal_type(x)  # revealed: int | str
```

### Generic example

```py
from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T")

IntAndT = TypeAliasType("IntAndT", tuple[int, T], type_params=(T,))

def f(x: IntAndT[str]) -> None:
    # TODO: This should be `tuple[int, str]`
    reveal_type(x)  # revealed: Unknown
```

### Generic value binds type variables to alias definition

```py
from typing import Generic
from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T", bound=int)
A = TypeAliasType("A", tuple[T], type_params=(T,))

S = TypeVar("S", bound=tuple[int])

class C(Generic[S]):
    pass

x: C[A]
```

### Error cases

#### Name is not a string literal

```py
from typing_extensions import TypeAliasType

def get_name() -> str:
    return "IntOrStr"

# error: [invalid-type-alias-type] "The first argument to `TypeAliasType` must be a string literal"
IntOrStr = TypeAliasType(get_name(), int | str)
```

#### Name does not match variable

```py
from typing import Union
from typing_extensions import TypeAliasType

# error: [mismatched-type-name] "The name passed to `TypeAliasType` must match the variable it is assigned to: Expected "IntOrStr", got "WrongName""
IntOrStr = TypeAliasType("WrongName", Union[int, str])
reveal_type(IntOrStr)  # revealed: TypeAliasType
```

#### Not a simple variable assignment

`TypeAliasType` must be used in a simple variable assignment. Using it as a standalone expression or
in a tuple unpacking is not supported.

```py
from typing_extensions import TypeAliasType

# error: [invalid-type-alias-type] "A `TypeAliasType` definition must be a simple variable assignment"
TypeAliasType("IntOrStr", "int | str")
```

### Mutually recursive `TypeAliasType` definitions

Mutually recursive type aliases created via the `TypeAliasType` constructor should not cause the
type checker to hang. The value type is computed lazily to break cycles.

```py
from typing_extensions import TypeAliasType, Union

A = TypeAliasType("A", Union[str, "B"])
B = TypeAliasType("B", list[A])

def f(x: A) -> None:
    reveal_type(x)  # revealed: str | list[A]

def g(x: B) -> None:
    reveal_type(x)  # revealed: list[A]
```

## Cyclic aliases

### Self-referential

```py
type OptNestedInt = int | tuple[OptNestedInt, ...] | None

def f(x: OptNestedInt) -> None:
    reveal_type(x)  # revealed: int | tuple[OptNestedInt, ...] | None
    if x is not None:
        reveal_type(x)  # revealed: int | tuple[OptNestedInt, ...]

type RecursiveList = list[RecursiveList]

def g(x: RecursiveList):
    reveal_type(x[0])  # revealed: list[RecursiveList]
```

### Invalid self-referential

```py
# TODO emit a diagnostic on these two lines
type IntOr = int | IntOr
type OrInt = OrInt | int

def f(x: IntOr, y: OrInt):
    reveal_type(x)  # revealed: int
    reveal_type(y)  # revealed: int
    if not isinstance(x, int):
        reveal_type(x)  # revealed: Never
    if not isinstance(y, int):
        reveal_type(y)  # revealed: Never

# error: [cyclic-type-alias-definition] "Cyclic definition of `Itself`"
type Itself = Itself

def foo(
    # this is a very strange thing to do, but this is a regression test to ensure it doesn't panic
    Itself: Itself,
):
    x: Itself
    reveal_type(Itself)  # revealed: Divergent

# A type alias defined with invalid recursion behaves as a dynamic type.
foo(42)
foo("hello")

# error: [cyclic-type-alias-definition] "Cyclic definition of `A`"
type A = B
# error: [cyclic-type-alias-definition] "Cyclic definition of `B`"
type B = A

def bar(B: B):
    x: B
    reveal_type(B)  # revealed: Divergent

# error: [cyclic-type-alias-definition] "Cyclic definition of `G`"
type G[T] = G[T]
# error: [cyclic-type-alias-definition] "Cyclic definition of `H`"
type H[T] = I[T]
# error: [cyclic-type-alias-definition] "Cyclic definition of `I`"
type I[T] = H[T]

# It's not possible to create an element of this type, but it's not an error for now
type DirectRecursiveList[T] = list[DirectRecursiveList[T]]

# TODO: this should probably be a cyclic-type-alias-definition error
type Foo[T] = list[T] | Bar[T]
type Bar[T] = int | Foo[T]

def _(x: Bar[int]):
    reveal_type(x)  # revealed: int | list[int]
```

### With legacy generic

```py
from typing import Generic, TypeVar

T = TypeVar("T")

type Alias = list["Alias"] | int

class A(Generic[T]):
    attr: T

class B(A[Alias]):
    pass

def f(b: B):
    reveal_type(b)  # revealed: B
    reveal_type(b.attr)  # revealed: list[Alias] | int
```

### Mutually recursive

```py
type A = tuple[B] | None
type B = tuple[A] | None

def f(x: A):
    if x is not None:
        reveal_type(x)  # revealed: tuple[B]
        y = x[0]
        if y is not None:
            reveal_type(y)  # revealed: tuple[A]

def g(x: A | B):
    reveal_type(x)  # revealed: tuple[B] | None

from ty_extensions import Intersection

def h(x: Intersection[A, B]):
    reveal_type(x)  # revealed: tuple[B] | None
```

### Self-recursive callable type

```py
from typing import Callable

type C = Callable[[], C | None]

def _(x: C):
    reveal_type(x)  # revealed: () -> C | None
```

### Subtyping of materializations of cyclic aliases

```py
from ty_extensions import static_assert, is_subtype_of, Bottom, Top

type JsonValue = None | JsonDict
type JsonDict = dict[str, JsonValue]

static_assert(is_subtype_of(Top[JsonDict], Top[JsonDict]))
static_assert(is_subtype_of(Top[JsonDict], Bottom[JsonDict]))
static_assert(is_subtype_of(Bottom[JsonDict], Bottom[JsonDict]))
static_assert(is_subtype_of(Bottom[JsonDict], Top[JsonDict]))
```

### Equivalence of top materializations of mutually recursive invariant aliases

```py
from typing import Callable
from ty_extensions import static_assert, is_equivalent_to, is_subtype_of, Top

class Box[T]:
    pass

type A = Callable[[B], None]
type B = Callable[[A], None]

static_assert(is_equivalent_to(Top[Box[A]], Top[Box[B]]))
static_assert(is_subtype_of(Top[Box[A]], Top[Box[B]]))
static_assert(is_subtype_of(Top[Box[B]], Top[Box[A]]))
```

### Assignment through recursive aliases

```py
from __future__ import annotations

type JSON = str | int | float | bool | list[JSON] | list[JSON_OBJECT] | dict[str, JSON] | None
type JSON_OBJECT = dict[str, JSON]

x: JSON_OBJECT = {"hello": 23}

def f() -> JSON_OBJECT:
    return {"hello": 23}
```

### Recursive dict alias in method return

```py
from __future__ import annotations
from dataclasses import dataclass

type NodeDict = dict[str, str | list[NodeDict]]

@dataclass
class Node:
    label: str
    children: list[Node]

    def to_dict(self) -> NodeDict:
        return {"label": self.label, "children": [child.to_dict() for child in self.children]}
```

### Cyclic defaults

```py
from typing_extensions import Protocol, TypeVar

T = TypeVar("T", default="C", covariant=True)

class P(Protocol[T]):
    pass

class C(P[T]):
    pass

reveal_type(C[int]())  # revealed: C[int]
reveal_type(C())  # revealed: C[C[Divergent]]
```

### Union inside generic

#### With old-style union

```py
from typing import Union

type A = list[Union["A", str]]

def f(x: A):
    reveal_type(x)  # revealed: list[A | str]
    for item in x:
        reveal_type(item)  # revealed: list[A | str] | str
```

#### With new-style union

```py
type A = list[A | str]

def f(x: A):
    reveal_type(x)  # revealed: list[A | str]
    for item in x:
        reveal_type(item)  # revealed: list[A | str] | str
```

#### With Optional

```py
from typing import Optional, Union

type A = list[Optional[Union["A", str]]]

def f(x: A):
    reveal_type(x)  # revealed: list[A | str | None]
    for item in x:
        reveal_type(item)  # revealed: list[A | str | None] | str | None
```

### Tuple comparison

```py
type X = tuple[X, int]

def _(x: X):
    reveal_type(x is x)  # revealed: bool
```

### Recursive invariant

```py
type X = dict[str, X]
type Y = X | str | dict[str, Y]

def _(y: Y):
    if isinstance(y, dict):
        reveal_type(y)  # revealed: dict[str, X] | dict[str, Y]
```

### Recursive alias with tuple - stack overflow test (issue 2470)

This test case used to cause a stack overflow. The returned type `list[int]` is not assignable to
`RecursiveT = int | tuple[RecursiveT, ...]`, so we get an error.

```py
type RecursiveT = int | tuple[RecursiveT, ...]

def foo(a: int, b: int) -> RecursiveT:
    some_intermediate_var = (a, b)
    # error: [invalid-return-type] "Return type does not match returned value: expected `RecursiveT`, found `list[int]`"
    return list(some_intermediate_var)
```

### Recursive `TypeIs` and `TypeGuard` aliases don't stack overflow

```py
from typing_extensions import TypeGuard, TypeIs
from collections.abc import Callable

type RecursiveIs = TypeIs[RecursiveIs]  # error: [cyclic-type-alias-definition]
type RecursiveGuard = TypeGuard[RecursiveGuard]

type AliasIs = RecursiveIs  # error: [cyclic-type-alias-definition]
type AliasGuard = RecursiveGuard

type CallableIs = TypeIs[Callable[[], CallableIs]]
type CallableGuard = TypeGuard[Callable[[], CallableGuard]]

reveal_type(CallableIs)  # revealed: TypeAliasType
reveal_type(CallableGuard)  # revealed: TypeAliasType
```

### Recursive alias in binary operators doesn't stack overflow

```py
from typing import reveal_type

type A = int | A

def foo(x: A):
    reveal_type(x + 1)  # revealed: int
    reveal_type(1 + x)  # revealed: int
```
