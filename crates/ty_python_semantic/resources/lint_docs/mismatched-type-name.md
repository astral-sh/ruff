## What it does

Checks for functional typing definitions whose declared name does not match
the variable they are assigned to.

## Why is this bad?

Constructors like `TypeVar`, `ParamSpec`, `NewType`, `NamedTuple`,
`TypedDict`, and `TypeAliasType` all take a name argument that is
normally expected to match the assigned variable. A mismatch is usually a
typo and makes later diagnostics harder to understand.

## Default level

This rule is a warning by default because ty can usually recover and
continue understanding the resulting type.

## Examples

```python
from typing import NewType, ParamSpec, TypeVar
from typing_extensions import TypedDict

T = TypeVar("U")  # error: [mismatched-type-name]
P = ParamSpec("Q")  # error: [mismatched-type-name]
UserId = NewType("Id", int)  # error: [mismatched-type-name]
Movie = TypedDict("Film", {"title": str})  # error: [mismatched-type-name]
```
