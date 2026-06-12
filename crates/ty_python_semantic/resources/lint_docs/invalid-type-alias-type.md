## What it does

Checks for the creation of invalid `TypeAliasType`s

## Why is this bad?

There are several requirements that you must follow when creating a `TypeAliasType`.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from typing import TypeAliasType


def get_name() -> str:
    return "NewAlias"


IntOrStr = TypeAliasType("IntOrStr", int | str)  # okay
# TypeAliasType name must be a string literal
NewAlias = TypeAliasType(get_name(), int)  # error
```
