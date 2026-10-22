## What it does

Checks for circular type alias definitions.

## Why is it bad?

Recursive aliases are valid when recursive references occur inside another type, such as
`list[Tree]`. An alias cannot expand directly to itself or include itself as a union member. This
applies to implicit type aliases, aliases annotated with `TypeAlias`, `type` statements, and aliases
created with `TypeAliasType`.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from typing import TypeAlias, TypeAliasType, Union

type Itself = Itself  # error

type A = B  # error
type B = A  # error

type IntOr = int | IntOr  # error

Cycle = TypeAliasType("Cycle", "Cycle")  # error

LegacyCycle: TypeAlias = "int | LegacyCycle"  # error

ImplicitCycle = Union[int, "ImplicitCycle"]  # error
value: ImplicitCycle

type Tree = int | list[Tree]  # valid recursive alias
```
