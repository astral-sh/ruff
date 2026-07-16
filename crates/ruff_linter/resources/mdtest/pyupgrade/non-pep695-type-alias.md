# `non-pep695-type-alias` (`UP040`)

## `TypeVar` defaults before Python 3.13

`typing_extensions` backports the `default` argument to Python 3.12 and earlier, but the PEP-695
syntax enforced by the rule is only available on 3.13 and later, so we have to avoid a diagnostic in
both of these cases.

```toml
target-version = "py312"

[lint]
select = ["UP040"]
```

### `TypeAlias`

```py
from typing import TypeAlias
from typing_extensions import TypeVar

T = TypeVar("T", default=int)
Alias: TypeAlias = list[T]
```

### `TypeAliasType`

```py
from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T", default=int)
Alias = TypeAliasType("Alias", list[T], type_params=(T,))
```
