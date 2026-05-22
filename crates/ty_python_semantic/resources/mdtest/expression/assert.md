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

## Assert statements with explicit falsy literals

These assertions are commonly used as unreachable code markers or abstract method bodies, so we
don't emit `redundant-assert` for them.

```py
assert False
assert 0
assert 0.0
assert 0j
assert None
assert ""
assert b""
assert ()
assert []
assert {}
```

## Assert statements for version and platform checks

These assertions are often used as compatibility guards for type checkers or generated modules, so
we don't emit `redundant-assert` even when they're known to be always true or always false for the
current target.

```py
import sys

assert sys.platform == "linux"
assert sys.version_info >= (3, 10)
assert sys.version_info[:2] == (3, 10)
assert sys.version_info.major == 3
```

## Assert statements for runtime type checks

Runtime type checks are commonly used as defensive validation and narrowing assertions. They are
left alone even if the type checker can prove that they always pass or fail.

```py
class Style: ...

style = Style()
assert isinstance(style, Style)
assert isinstance(style, str)
assert style is None or isinstance(style, Style)
```
