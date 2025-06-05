# PEP 695 type aliases

PEP 695 type aliases are only available in Python 3.12 and later:

```toml
[environment]
python-version = "3.12"
```

## Basic

```py
type IntOrStr = int | str

reveal_type(IntOrStr)  # revealed: typing.TypeAliasType
reveal_type(IntOrStr.__name__)  # revealed: Literal["IntOrStr"]

x: IntOrStr = 1

reveal_type(x)  # revealed: Literal[1]

def f() -> None:
    reveal_type(x)  # revealed: IntOrStr
```

## Type properties

### Equivalence

```py
from ty_extensions import static_assert, is_equivalent_to

type IntOrStr = int | str
type StrOrInt = str | int

static_assert(is_equivalent_to(IntOrStr, IntOrStr))
static_assert(is_equivalent_to(IntOrStr, StrOrInt))

type Rec1 = tuple[Rec1, int]
type Rec2 = tuple[Rec2, int]

type Other = tuple[Other, str]

static_assert(is_equivalent_to(Rec1, Rec2))
static_assert(not is_equivalent_to(Rec1, Other))

type Cycle1A = tuple[Cycle1B, int]
type Cycle1B = tuple[Cycle1A, str]

type Cycle2A = tuple[Cycle2B, int]
type Cycle2B = tuple[Cycle2A, str]

static_assert(is_equivalent_to(Cycle1A, Cycle2A))
static_assert(is_equivalent_to(Cycle1B, Cycle2B))
static_assert(not is_equivalent_to(Cycle1A, Cycle1B))
static_assert(not is_equivalent_to(Cycle1A, Cycle2B))

# type Cycle3A = tuple[Cycle3B] | None
# type Cycle3B = tuple[Cycle3A] | None

# static_assert(is_equivalent_to(Cycle3A, Cycle3A))
# static_assert(is_equivalent_to(Cycle3A, Cycle3B))
```

### Assignability

```py
type IntOrStr = int | str

x1: IntOrStr = 1
x2: IntOrStr = "1"
x3: IntOrStr | None = None

def _(int_or_str: IntOrStr) -> None:
    # TODO: those should not be errors
    x3: int | str = int_or_str  # error: [invalid-assignment]
    x4: int | str | None = int_or_str  # error: [invalid-assignment]
    x5: int | str | None = int_or_str or None  # error: [invalid-assignment]
```

### Narrowing (intersections)

```py
class P: ...
class Q: ...

type EitherOr = P | Q

def _(x: EitherOr) -> None:
    if isinstance(x, P):
        reveal_type(x)  # revealed: P
    elif isinstance(x, Q):
        reveal_type(x)  # revealed: Q & ~P
    else:
        # TODO: This should be Never
        reveal_type(x)  # revealed: EitherOr & ~P & ~Q
```

### Fully static

```py
from typing import Any
from ty_extensions import static_assert, is_fully_static

type IntOrStr = int | str
type RecFullyStatic = int | tuple[RecFullyStatic]

static_assert(is_fully_static(IntOrStr))
static_assert(is_fully_static(RecFullyStatic))

type IntOrAny = int | Any
type RecNotFullyStatic = Any | tuple[RecNotFullyStatic]

static_assert(not is_fully_static(IntOrAny))
static_assert(not is_fully_static(RecNotFullyStatic))
```

## `__value__` attribute

```py
type IntOrStr = int | str

reveal_type(IntOrStr.__value__)  # revealed: @Todo(Support for `typing.TypeAlias`)
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
    reveal_type(x)  # revealed: IntOrStrOrBytes
```

## Aliased type aliases

```py
type IntOrStr = int | str
MyIntOrStr = IntOrStr

x: MyIntOrStr = 1

# error: [invalid-assignment]
y: MyIntOrStr = None
```

## Generic type aliases

```py
type ListOrSet[T] = list[T] | set[T]
reveal_type(ListOrSet.__type_params__)  # revealed: tuple[TypeVar | ParamSpec | TypeVarTuple, ...]
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

reveal_type(IntOrStr)  # revealed: typing.TypeAliasType

reveal_type(IntOrStr.__name__)  # revealed: Literal["IntOrStr"]

def f(x: IntOrStr) -> None:
    reveal_type(x)  # revealed: IntOrStr
```

### Generic example

```py
from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T")

IntAnd = TypeAliasType("IntAndT", tuple[int, T], type_params=(T,))

def f(x: IntAnd[str]) -> None:
    reveal_type(x)  # revealed: @Todo(Generic PEP-695 type alias)
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

## Recursive type aliases

```py
type Recursive = dict[str, "Recursive"]

# TODO: this should not be an error
r: Recursive = {"key": {}}  # error: [invalid-assignment]
```
