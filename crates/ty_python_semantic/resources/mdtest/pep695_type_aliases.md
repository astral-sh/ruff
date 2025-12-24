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

### Error cases

#### Name is not a string literal

```py
from typing_extensions import TypeAliasType

def get_name() -> str:
    return "IntOrStr"

# error: [invalid-type-alias-type] "The name of a `typing.TypeAlias` must be a string literal"
IntOrStr = TypeAliasType(get_name(), int | str)
```

## Cyclic aliases

### Self-referential

```py
type OptNestedInt = int | tuple[OptNestedInt, ...] | None

def f(x: OptNestedInt) -> None:
    reveal_type(x)  # revealed: int | tuple[OptNestedInt, ...] | None
    if x is not None:
        reveal_type(x)  # revealed: int | tuple[OptNestedInt, ...]
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
    # TODO: should be `int | list[int]`
    reveal_type(x)  # revealed: int | list[int] | Any
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
        reveal_type(item)  # revealed: list[Any | str] | str
```

#### With new-style union

```py
type A = list["A" | str]

def f(x: A):
    reveal_type(x)  # revealed: list[A | str]
    for item in x:
        reveal_type(item)  # revealed: list[Any | str] | str
```

#### With Optional

```py
from typing import Optional, Union

type A = list[Optional[Union["A", str]]]

def f(x: A):
    reveal_type(x)  # revealed: list[A | str | None]
    for item in x:
        reveal_type(item)  # revealed: list[Any | str | None] | str | None
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
