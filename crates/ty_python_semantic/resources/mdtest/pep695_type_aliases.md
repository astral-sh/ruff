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
    reveal_type(x)  # revealed: int | str
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
    reveal_type(x)  # revealed: int | str
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
