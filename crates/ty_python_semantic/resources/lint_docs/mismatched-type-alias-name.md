## What it does

Checks whether the name passed to `TypeAliasType` differs from the assigned variable.

## Why is this bad?

A mismatched name can be a typo and can make a type alias's runtime representation confusing.

## Examples

```python
from typing_extensions import TypeAliasType

UserId = TypeAliasType("Id", int)  # error: [mismatched-type-alias-name] "Expected "UserId", got "Id""
```
