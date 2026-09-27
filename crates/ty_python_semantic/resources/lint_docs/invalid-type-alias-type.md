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
from typing import TypeAliasType, TypeVar


def get_name() -> str:
    return "NewAlias"


IntOrStr = TypeAliasType("IntOrStr", int | str)  # okay
# TypeAliasType name must be a string literal
NewAlias = TypeAliasType(get_name(), int)  # error

T = TypeVar("T")
GenericAlias = TypeAliasType("GenericAlias", list[T], type_params=(T,))  # okay
# TypeAliasType type parameters must be type variables
InvalidAlias = TypeAliasType("InvalidAlias", list[T], type_params=(list[T],))  # error
```
