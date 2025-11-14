# PEP 613 type aliases

We do not support PEP 613 type aliases yet. For now, just make sure that we don't panic:

```py
from typing import TypeAlias
from types import UnionType

RecursiveTuple: TypeAlias = tuple[int | "RecursiveTuple", str]

def _(rec: RecursiveTuple):
    reveal_type(rec)  # revealed: tuple[Divergent, str]

RecursiveHomogeneousTuple: TypeAlias = tuple[int | "RecursiveHomogeneousTuple", ...]

def _(rec: RecursiveHomogeneousTuple):
    reveal_type(rec)  # revealed: tuple[Divergent, ...]

ClassInfo: TypeAlias = type | UnionType | tuple["ClassInfo", ...]
reveal_type(ClassInfo)  # revealed: types.UnionType

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
