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

## Subscripted generic alias in union

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")

Alias1: TypeAlias = list[T] | set[T]
MyAlias: TypeAlias = int | Alias1[str]

def _(x: MyAlias):
    # TODO: int | list[str] | set[str]
    reveal_type(x)  # revealed: int | @Todo(Specialization of union type alias)
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
from typing import TypeAlias
from types import UnionType

RecursiveTuple: TypeAlias = tuple[int | "RecursiveTuple", str]

def _(rec: RecursiveTuple):
    # TODO should be `RecursiveTuple`
    reveal_type(rec)  # revealed: Divergent

RecursiveHomogeneousTuple: TypeAlias = tuple[int | "RecursiveHomogeneousTuple", ...]

def _(rec: RecursiveHomogeneousTuple):
    # TODO should be `RecursiveHomogeneousTuple`
    reveal_type(rec)  # revealed: tuple[Divergent, ...] | Divergent

ClassInfo: TypeAlias = type | UnionType | tuple["ClassInfo", ...]
# TODO should be `types.UnionType`
reveal_type(ClassInfo)  # revealed: types.UnionType | types.UnionType

def my_isinstance(obj: object, classinfo: ClassInfo) -> bool:
    reveal_type(classinfo)  # revealed: type | UnionType | tuple[Divergent, ...]
    return isinstance(obj, classinfo)

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
