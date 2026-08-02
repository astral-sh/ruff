# `boolean-type-hint-positional-argument` (`FBT001`)

```toml
[lint]
preview = true
select = ["FBT001"]
```

## `Literal` annotations

In preview, `typing.Literal` annotations that include both `True` and `False` as variants are flagged, e.g. `Literal[True, False]`.

```py
from typing import Literal

def func_literal_both(flag: Literal[True, False]):  # error: [boolean-type-hint-positional-argument]
    pass
```

A `Literal` containing only `True` or only `False` is not flagged.

```py
from typing import Literal

def func_literal_true(flag: Literal[True]):
    pass

def func_literal_false(flag: Literal[False]):
    pass
```

Other literal variants don't stop annotation from being flagged.

```py
from typing import Literal

def func_literal_optional(flag: Literal[True, False, None, "hello"]):  # error: [boolean-type-hint-positional-argument]
    pass
```

`True` and `False` are also collected across separate `Literal` members joined by `|`.

```py
from typing import Literal

def func_literal_split_union_optional(flag: Literal[True] | Literal[False] | None):  # error: [boolean-type-hint-positional-argument]
    pass
```

Support nested `Literal` expressions.

```py
from typing import Literal

def func_literal_nested(flag: Literal[True, Literal[False]]):  # error: [boolean-type-hint-positional-argument]
    pass
```
