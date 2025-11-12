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

## String literal in RHS

```py
from typing import TypeAlias

IntOrStr: TypeAlias = "int | str"

def _(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
```

## Cyclic

```py
from typing import TypeAlias

RecursiveTuple: TypeAlias = tuple[int | "RecursiveTuple", str]

def _(rec: RecursiveTuple):
    reveal_type(rec)  # revealed: tuple[Divergent, str]

RecursiveHomogeneousTuple: TypeAlias = tuple[int | "RecursiveHomogeneousTuple", ...]

def _(rec: RecursiveHomogeneousTuple):
    reveal_type(rec)  # revealed: tuple[Divergent, ...]
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

## RHS is required

```py
from typing import TypeAlias

# error: [invalid-type-form]
Empty: TypeAlias
```
