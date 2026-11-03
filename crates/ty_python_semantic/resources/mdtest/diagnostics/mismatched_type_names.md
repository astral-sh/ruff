# Selecting rules for mismatched type names

## Existing configuration

The deprecated `mismatched-type-name` setting applies to all eight rules. A setting for an
individual kind of type takes precedence within the same table.

```toml
[rules]
mismatched-type-name = "ignore"
mismatched-typevar-name = "error"
```

```py
from enum import IntFlag
from typing import NamedTuple, NewType, ParamSpec, TypedDict, TypeVar
from typing_extensions import TypeAliasType, TypeVarTuple

Flags = IntFlag("Options", "READ WRITE")  # ok
Point = NamedTuple("Vector", [("x", int)])  # ok
UserId = NewType("Id", int)  # ok
P = ParamSpec("Q")  # ok
Number = TypeAliasType("Num", int)  # ok
Movie = TypedDict("Film", {"title": str})  # ok
Ts = TypeVarTuple("Us")  # ok

# snapshot: mismatched-typevar-name
T = TypeVar("U")
```

```snapshot
error[mismatched-typevar-name]: The name passed to `TypeVar` must match the variable it is assigned to
  --> src/mdtest_snippet.py:14:13
   |
14 | T = TypeVar("U")
   |             ^^^ Expected "T", got "U"
```

## Existing ignore comments

An ignore using the old name can suppress any of the eight diagnostics. A rule-specific ignore only
suppresses mismatches for its own kind of type.

```py
from collections import namedtuple
from typing import NewType, TypeVar

Point = namedtuple("Vector", "x y")  # ty: ignore[mismatched-type-name]
T = TypeVar("U")  # type: ignore[ty:mismatched-type-name]

# error: [mismatched-newtype-name]
# error: [unused-ignore-comment]
UserId = NewType("Id", int)  # ty: ignore[mismatched-typevar-name]
```

An unused ignore reports the name that appeared in the comment.

```py
# error: [unused-ignore-comment] "Unused `ty: ignore` directive: 'mismatched-type-name'"
missing  # ty: ignore[mismatched-type-name, unresolved-reference]
```
