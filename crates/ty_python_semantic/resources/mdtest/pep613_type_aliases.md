# PEP 613 type aliases

## No panics

We do not fully support PEP 613 type aliases yet. For now, just make sure that we don't panic:

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

# TODO: we should emit `[invalid-argument-type]` here
# (the alias is a `@Todo` because it's imported from another file)
f(Unrelated())
```
