# PEP 613 type aliases

PEP 613 type aliases are simple assignment statements, annotated with `typing.TypeAlias` to mark
them as a type alias. At runtime, they behave the same as implicit type aliases. Our support for
them is currently the same as for implicit type aliases, but we don't reproduce the full
implicit-type-alias test suite here, just some particularly interesting cases.

## Basic

### as `TypeAlias`

```py
from typing import TypeAlias

IntOrStr: TypeAlias = int | str

def _(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
```

### as `typing.TypeAlias`

```py
import typing

IntOrStr: typing.TypeAlias = int | str

def _(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
```

## Can be used as value

Because PEP 613 type aliases are just annotated assignments, they can be used as values, like a
legacy type expression (and unlike a PEP 695 type alias). We might prefer this wasn't allowed, but
people do use it.

```py
from typing import TypeAlias

MyExc: TypeAlias = Exception

try:
    raise MyExc("error")
except MyExc as e:
    reveal_type(e)  # revealed: Exception
```

## Can inherit from an alias

```py
from typing import TypeAlias
from ty_extensions import is_subtype_of, static_assert

MyList: TypeAlias = list["int"]

class Foo(MyList): ...

static_assert(is_subtype_of(Foo, list[int]))
```

## Cannot inherit from a stringified alias

```py
from typing import TypeAlias

MyList: TypeAlias = "list[int]"

# error: [invalid-base] "Invalid class base with type `str`"
class Foo(MyList): ...
```

## Unknown type in PEP 604 union

If we run into an unknown type in a PEP 604 union in the right-hand side of a PEP 613 type alias, we
still understand it as a union type, just with an unknown element.

```py
from typing import TypeAlias
from nonexistent import unknown_type  # error: [unresolved-import]

MyAlias: TypeAlias = int | unknown_type | str

def _(x: MyAlias):
    reveal_type(x)  # revealed: int | Unknown | str
```

## Callable type in union

```py
from typing import TypeAlias, Callable

MyAlias: TypeAlias = int | Callable[[str], int]

def _(x: MyAlias):
    reveal_type(x)  # revealed: int | ((str, /) -> int)
```

## Generic aliases

A more comprehensive set of tests can be found in
[`implicit_type_aliases.md`](./implicit_type_aliases.md). If the implementations ever diverge, we
may need to duplicate more tests here.

### Basic

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")

MyList: TypeAlias = list[T]
ListOrSet: TypeAlias = list[T] | set[T]

reveal_type(MyList)  # revealed: <class 'list[T]'>
reveal_type(ListOrSet)  # revealed: <types.UnionType special-form 'list[T] | set[T]'>

def _(list_of_int: MyList[int], list_or_set_of_str: ListOrSet[str]):
    reveal_type(list_of_int)  # revealed: list[int]
    reveal_type(list_or_set_of_str)  # revealed: list[str] | set[str]
```

### Stringified generic alias

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")
U = TypeVar("U")

TotallyStringifiedPEP613: TypeAlias = "dict[T, U]"
TotallyStringifiedPartiallySpecialized: TypeAlias = "TotallyStringifiedPEP613[U, int]"

def f(x: "TotallyStringifiedPartiallySpecialized[str]"):
    reveal_type(x)  # revealed: @Todo(Generic stringified PEP-613 type alias)
```

## Subscripted generic alias in union

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")

Alias1: TypeAlias = list[T] | set[T]
MyAlias: TypeAlias = int | Alias1[str]

def _(x: MyAlias):
    reveal_type(x)  # revealed: int | list[str] | set[str]
```

## Typevar-specialized dynamic types

We still recognize type aliases as being generic if a symbol of a dynamic type is explicitly
specialized with a type variable:

```py
from typing import TypeVar, TypeAlias

from unknown_module import UnknownClass  # type: ignore

T = TypeVar("T")

MyAlias1: TypeAlias = UnknownClass[T] | None

def _(a: MyAlias1[int]):
    reveal_type(a)  # revealed: Unknown | None
```

This also works with multiple type arguments:

```py
U = TypeVar("U")
V = TypeVar("V")

MyAlias2: TypeAlias = UnknownClass[T, U, V] | int

def _(a: MyAlias2[int, str, bytes]):
    reveal_type(a)  # revealed: Unknown | int
```

If we specialize with fewer or more type arguments than expected, we emit an error:

```py
def _(
    # error: [invalid-type-arguments] "No type argument provided for required type variable `V`"
    too_few: MyAlias2[int, str],
    # error: [invalid-type-arguments] "Too many type arguments: expected 3, got 4"
    too_many: MyAlias2[int, str, bytes, float],
): ...
```

We can also reference these type aliases from other type aliases:

```py
MyAlias3: TypeAlias = MyAlias1[str] | MyAlias2[int, str, bytes]

def _(c: MyAlias3):
    reveal_type(c)  # revealed: Unknown | None | int
```

Here, we test some other cases that might involve `@Todo` types, which also need special handling:

```py
from typing_extensions import Callable, Concatenate, TypeAliasType

MyAlias4: TypeAlias = Callable[Concatenate[dict[str, T], ...], list[U]]

def _(c: MyAlias4[int, str]):
    # TODO: should be (int, / ...) -> str
    reveal_type(c)  # revealed: Unknown

T = TypeVar("T")

MyList = TypeAliasType("MyList", list[T], type_params=(T,))

MyAlias5 = Callable[[MyList[T]], int]

def _(c: MyAlias5[int]):
    # TODO: should be (list[int], /) -> int
    reveal_type(c)  # revealed: (Unknown, /) -> int

K = TypeVar("K")
V = TypeVar("V")

MyDict = TypeAliasType("MyDict", dict[K, V], type_params=(K, V))

MyAlias6 = Callable[[MyDict[K, V]], int]

def _(c: MyAlias6[str, bytes]):
    # TODO: should be (dict[str, bytes], /) -> int
    reveal_type(c)  # revealed: (Unknown, /) -> int

ListOrDict: TypeAlias = MyList[T] | dict[str, T]

def _(x: ListOrDict[int]):
    # TODO: should be list[int] | dict[str, int]
    reveal_type(x)  # revealed: Unknown | dict[str, int]

MyAlias7: TypeAlias = Callable[Concatenate[T, ...], None]

def _(c: MyAlias7[int]):
    # TODO: should be (int, / ...) -> None
    reveal_type(c)  # revealed: Unknown
```

## Imported

`alias.py`:

```py
from typing import TypeAlias

MyAlias: TypeAlias = int | str
```

`main.py`:

```py
from alias import MyAlias

def _(x: MyAlias):
    reveal_type(x)  # revealed: int | str
```

## String literal in right-hand side

```py
from typing import TypeAlias

IntOrStr: TypeAlias = "int | str"

def _(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
```

## Cyclic

```py
from typing import TypeAlias, TypeVar, Union
from types import UnionType

RecursiveTuple: TypeAlias = tuple[int | "RecursiveTuple", str]

def _(rec: RecursiveTuple):
    # TODO should be `tuple[int | RecursiveTuple, str]`
    reveal_type(rec)  # revealed: tuple[Divergent, str]

RecursiveHomogeneousTuple: TypeAlias = tuple[int | "RecursiveHomogeneousTuple", ...]

def _(rec: RecursiveHomogeneousTuple):
    # TODO should be `tuple[int | RecursiveHomogeneousTuple, ...]`
    reveal_type(rec)  # revealed: tuple[Divergent, ...]

ClassInfo: TypeAlias = type | UnionType | tuple["ClassInfo", ...]
reveal_type(ClassInfo)  # revealed: <types.UnionType special-form 'type | UnionType | tuple[Divergent, ...]'>

def my_isinstance(obj: object, classinfo: ClassInfo) -> bool:
    # TODO should be `type | UnionType | tuple[ClassInfo, ...]`
    reveal_type(classinfo)  # revealed: type | UnionType | tuple[Divergent, ...]
    return isinstance(obj, classinfo)

K = TypeVar("K")
V = TypeVar("V")
NestedDict: TypeAlias = dict[K, Union[V, "NestedDict[K, V]"]]

def _(nested: NestedDict[str, int]):
    # TODO should be `dict[str, int | NestedDict[str, int]]`
    reveal_type(nested)  # revealed: dict[@Todo(specialized recursive generic type alias), Divergent]

my_isinstance(1, int)
my_isinstance(1, int | str)
my_isinstance(1, (int, str))
my_isinstance(1, (int, (str, float)))
my_isinstance(1, (int, (str | float)))
# error: [invalid-argument-type]
my_isinstance(1, 1)
# TODO should be an invalid-argument-type error
my_isinstance(1, (int, (str, 1)))
```

## Conditionally imported on Python < 3.10

```toml
[environment]
python-version = "3.9"
```

```py
try:
    # error: [unresolved-import]
    from typing import TypeAlias
except ImportError:
    from typing_extensions import TypeAlias

MyAlias: TypeAlias = int

def _(x: MyAlias):
    reveal_type(x)  # revealed: int
```

## PEP-613 aliases in stubs are deferred

Although the right-hand side of a PEP-613 alias is a value expression, inference of this value is
deferred in a stub file, allowing for forward references:

`stub.pyi`:

```pyi
from typing import TypeAlias

MyAlias: TypeAlias = A | B

class A: ...
class B: ...
```

`module.py`:

```py
import stub

def f(x: stub.MyAlias): ...

f(stub.A())
f(stub.B())

class Unrelated: ...

# error: [invalid-argument-type]
f(Unrelated())
```

## Invalid position

`typing.TypeAlias` must be used as the sole annotation in an annotated assignment. Use in any other
context is an error.

```py
from typing import TypeAlias

# error: [invalid-type-form]
def _(x: TypeAlias):
    reveal_type(x)  # revealed: Unknown

# error: [invalid-type-form]
y: list[TypeAlias] = []
```

## Right-hand side is required

```py
from typing import TypeAlias

# error: [invalid-type-form]
Empty: TypeAlias
```
