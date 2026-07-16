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
# TODO
# snapshot: non-pep695-type-alias
Alias: TypeAlias = list[T]
```

```snapshot
error[UP040]: Type alias `Alias` uses `TypeAlias` annotation instead of the `type` keyword
 --> src/mdtest_snippet.py:7:1
  |
7 | Alias: TypeAlias = list[T]
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Use the `type` keyword
  |
6 | # snapshot: non-pep695-type-alias
  - Alias: TypeAlias = list[T]
7 + type Alias[T = int] = list[T]
  |
note: This is an unsafe fix and may change runtime behavior
```

### `TypeAliasType`

```py
from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T", default=int)
# TODO
# snapshot: non-pep695-type-alias
Alias = TypeAliasType("Alias", list[T], type_params=(T,))
```

```snapshot
error[UP040]: Type alias `Alias` uses `TypeAliasType` assignment instead of the `type` keyword
 --> src/mdtest_snippet.py:6:1
  |
6 | Alias = TypeAliasType("Alias", list[T], type_params=(T,))
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Use the `type` keyword
  |
5 | # snapshot: non-pep695-type-alias
  - Alias = TypeAliasType("Alias", list[T], type_params=(T,))
6 + type Alias[T = int] = list[T]
  |
```
