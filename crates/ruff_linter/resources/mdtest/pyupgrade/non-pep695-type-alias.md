# `non-pep695-type-alias` (`UP040`)

## `TypeVar` defaults before Python 3.13

`typing_extensions` backports the `default` argument to Python 3.12 and earlier, but the PEP-695
syntax enforced by the rule is only available on 3.13 and later, so we have to avoid a diagnostic in
both of these cases.

```toml
target-version = "py312"

[lint]
preview = true
select = ["non-pep695-type-alias"]
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

## `TypeVar` with unpacked keyword arguments

When a `TypeVar` uses unpacked keyword arguments (e.g. `**{"default": Any}`), the fix cannot
safely inline it into PEP 695 syntax and should not be offered.

```toml
target-version = "py314"
[lint]
preview = true
select = ["UP040", "UP046", "UP047"]
```

### `TypeAliasType` — no fix offered

```py
from typing import Any, TypeAliasType, TypeVar

T = TypeVar("T", **{"default": Any})
AnyList = TypeAliasType("AnyList", list[T], type_params=(T,))
```

### `TypeAlias` — no fix offered

```py
from typing import Any, TypeAlias, TypeVar

T = TypeVar("T", **{"default": Any})
Alias: TypeAlias = list[T]
```

### Mixed `TypeAlias` — only non-unpacked TypeVar converted

```py
from typing import Any, TypeAlias, TypeVar

T = TypeVar("T", **{"default": Any})
U = TypeVar("U")
MixedAlias: TypeAlias = tuple[T, U]
```

### Generic function — no fix offered

```py
from typing import Any, TypeVar

T = TypeVar("T", **{"default": Any})
U = TypeVar("U")

def f(first: T, second: U) -> tuple[T, U]:
    return first, second
```
