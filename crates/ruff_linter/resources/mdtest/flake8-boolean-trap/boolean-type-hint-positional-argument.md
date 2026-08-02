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

Support quoted type expressions.

```py
from typing import Literal

# Quoted base expression.
def func_literal_quoted(flag: "Literal[True, False]"):  # error: [boolean-type-hint-positional-argument]
    pass

# Quoted union.
def func_literal_quoted_union_leaf(flag: "int | Literal[True, False]"):  # error: [boolean-type-hint-positional-argument]
    pass


# Quoted union leaf.
def func_literal_quoted_union_leaf(flag: int | "Literal[True, False]"):  # error: [boolean-type-hint-positional-argument]
    pass

# Not supported, requires extending `traverse_union_and_optional`
# to support resolving quotes unions.
# Quoted union inside union.
def func_literal_quoted_nested_union(flag: str | "int | Literal[True, False]"):
    pass

```

Don't let shadowed `bool` to get in the way of flagging literals.

```py
from typing import Literal

def func_shadowed_bool():

  bool = int

  def func_literal_with_shadowed_bool(flag: Literal[True, False]):  # error: [boolean-type-hint-positional-argument]
      pass
```
