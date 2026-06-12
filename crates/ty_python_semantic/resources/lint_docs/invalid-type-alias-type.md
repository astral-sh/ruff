## What it does
Checks for the creation of invalid `TypeAliasType`s

## Why is this bad?
There are several requirements that you must follow when creating a `TypeAliasType`.

## Examples
```python
from typing import TypeAliasType

IntOrStr = TypeAliasType("IntOrStr", int | str)  # okay
NewAlias = TypeAliasType(get_name(), int)        # error: TypeAliasType name must be a string literal
```
