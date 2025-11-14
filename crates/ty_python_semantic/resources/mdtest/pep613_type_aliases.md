# PEP 613 type aliases

## No panics

We do not fully support PEP 613 type aliases yet. For now, just make sure that we don't panic:

```py
from typing import TypeAlias

RecursiveTuple: TypeAlias = tuple[int | "RecursiveTuple", str]

def _(rec: RecursiveTuple):
    reveal_type(rec)  # revealed: tuple[Divergent, str]

RecursiveHomogeneousTuple: TypeAlias = tuple[int | "RecursiveHomogeneousTuple", ...]

def _(rec: RecursiveHomogeneousTuple):
    reveal_type(rec)  # revealed: tuple[Divergent, ...]
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
