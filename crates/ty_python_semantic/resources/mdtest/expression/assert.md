## Condition with object that implements `__bool__` incorrectly

```py
class NotBoolable:
    __bool__: int = 3

# error: [unsupported-bool-conversion] "Boolean conversion is not supported for type `NotBoolable`"
assert NotBoolable()
```

## Redundant assert statements

```py
from typing import Any

class Style: ...

def get_style() -> Style:
    return Style()

style = get_style()
assert style is not None  # snapshot: redundant-assert
assert style is None  # error: [redundant-assert] "Assert condition is always false"

maybe_style: Style | None
assert maybe_style is not None

dynamic_style: Any
assert dynamic_style is not None
```

```snapshot
info[redundant-assert]: Assert condition is always true
 --> src/mdtest_snippet.py:9:8
  |
9 | assert style is not None  # snapshot: redundant-assert
  |        ^^^^^^^^^^^^^^^^^
  |
```

## Redundant assert statements after augmented assignment

```py
x = 6
x -= 1
assert x == 5  # error: [redundant-assert] "Assert condition is always true"
assert x == 6  # error: [redundant-assert] "Assert condition is always false"
```
