## What it does

Checks for circular type alias definitions.

## Why is it bad?

Recursive aliases are valid when recursive references occur inside another type, such as
`list[Tree]`. An alias cannot expand directly to itself or include itself as a union member. This
applies to both `type` statements and aliases created with `TypeAliasType`.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from typing import TypeAliasType

type Itself = Itself  # error

type A = B  # error
type B = A  # error

type IntOr = int | IntOr  # error

Cycle = TypeAliasType("Cycle", "Cycle")  # error

type Tree = int | list[Tree]  # valid recursive alias
```
