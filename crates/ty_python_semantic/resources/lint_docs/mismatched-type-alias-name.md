## What it does

Checks whether the name passed to `TypeAliasType` differs from the assigned variable.

## Why is this bad?

The name argument determines how the alias is displayed at runtime. A different name in the source
makes the alias harder to identify.

## Examples

```python
from typing_extensions import TypeAliasType

UserId = TypeAliasType("Id", int)  # error: [mismatched-type-alias-name]
```
