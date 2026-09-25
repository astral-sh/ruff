# Implicit boolean conversions

This opt-in rule detects truthiness checks that conflate `None` with other falsy values.

```toml
[rules]
implicit-bool-conversion = "warn"
```

## Missing values and zero

A valid zero must not be mistaken for a missing value. An explicit conversion is allowed when
truthiness is intentional.

```py
def process(limit: int | None):
    if not limit:  # snapshot: implicit-bool-conversion
        pass
    if limit is None:
        pass
    if not bool(limit):
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `int | None` conflates `None` with other falsy values
 --> src/mdtest_snippet.py:2:12
  |
2 |     if not limit:  # snapshot: implicit-bool-conversion
  |            ^^^^^ Both `None` and non-`None` values can be false
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

## Boolean contexts

```py
def check(items: list[int | None], count: int | None):
    if count:  # error: [implicit-bool-conversion]
        pass
    elif count:  # error: [implicit-bool-conversion]
        pass
    while count:  # error: [implicit-bool-conversion]
        break
    result = 1 if count else 0  # error: [implicit-bool-conversion]
    result = not count  # error: [implicit-bool-conversion]
    filtered = [x for x in items if x]  # error: [implicit-bool-conversion]
    match count:
        case _ if count:  # error: [implicit-bool-conversion]
            pass
    assert count  # error: [implicit-bool-conversion]
```

## Short-circuit expressions

Only operands that Python tests for truthiness are checked. The last operand can be any value when
the result is used as a value. Compound conditions report the individual operands.

```py
def check(items: list[int] | None, flag: bool, other: bool):
    value = flag and items
    value = flag or items
    value = items or []  # error: [implicit-bool-conversion]
    value = items and flag  # error: [implicit-bool-conversion]
    if items and flag:  # error: [implicit-bool-conversion]
        pass
    if flag and items:  # error: [implicit-bool-conversion]
        pass
    if not (items or flag):  # error: [implicit-bool-conversion]
        pass
    if flag and (other or items):  # error: [implicit-bool-conversion]
        pass
    if result := items and flag:  # error: [implicit-bool-conversion]
        pass
    if result := flag and items:  # error: [implicit-bool-conversion]
        pass
```

## Allowed types

Boolean literals, gradual types, and values narrowed to booleans are accepted.

```py
from typing_extensions import Any, Never, TypeVar

def check(flag: bool, dynamic: Any, unknown, missing: Never, optional: bool | None):
    if flag:
        pass
    if dynamic:
        pass
    if unknown:
        pass
    if missing:
        pass
    if optional is not None:
        if optional:
            pass

T = TypeVar("T", bound=bool)

def generic(flag: T):
    if flag:
        pass
```

## Custom truthiness

Optional objects with custom truthiness are checked.

```py
class Custom:
    def __bool__(self) -> bool:
        return True

def check(value: Custom | None):
    if value:  # error: [implicit-bool-conversion]
        pass
    if bool(value):
        pass
```

## Comparisons

Comparisons usually return booleans, but a custom comparison can return another type.

```py
class Custom:
    def __lt__(self, other: "Custom") -> int | None:
        return 1

def check(left: Custom, right: Custom):
    if left < right:  # error: [implicit-bool-conversion]
        pass
    result = left < right
    if bool(left < right):
        pass
```

## Constants

Non-optional literal values are allowed.

```py
if 0:
    pass
if "text":  # error: [redundant-condition]
    pass
if []:
    pass
```

## Unions and type parameters

Dynamic union members are excluded, even when a known member can be false.

```py
from typing import Any, TypeVar

def check(optional: bool | None, gradual: int | Any | None):
    if optional:  # error: [implicit-bool-conversion]
        pass
    if gradual:
        pass

T = TypeVar("T")

def generic(value: T):
    if value:
        pass
```

## Optional values with distinct falsy cases

Empty containers, zero, and `False` can all be confused with `None`. Dynamic element types do not
make a container itself dynamic.

```py
from typing import Any, Literal

def check(number: float | None, text: str | None, items: list[Any] | None):
    if number:  # error: [implicit-bool-conversion]
        pass
    if text:  # error: [implicit-bool-conversion]
        pass
    if items:  # error: [implicit-bool-conversion]
        pass
```

## Unambiguous presence checks

Always-truthy alternatives make a truthiness test an unambiguous presence check.

```py
import re
from typing import Literal, final

@final
class Present:
    pass

class AlwaysTrue:
    def __bool__(self) -> Literal[True]:
        return True

def check(
    match: re.Match[str] | None,
    present: Present | None,
    custom: AlwaysTrue | None,
    positive: Literal[1] | None,
    flag: Literal[True] | None,
):
    if match:
        pass
    if present:
        pass
    if custom:
        pass
    if positive:
        pass
    if flag:
        pass
```

## Non-optional and dynamic values

Ordinary truthiness checks remain valid. A union containing a dynamic alternative is excluded.

```py
from typing import Any

def check(items: list[str], number: int, text: str, dynamic: Any | None, unknown):
    if items:
        pass
    if number:
        pass
    if text:
        pass
    if dynamic:
        pass
    value = unknown if number else None
    if value:
        pass
```

## Nested scopes

```py
def check(items: list[int | None], count: int | None):
    predicate = lambda: not count  # error: [implicit-bool-conversion]
    filtered = (x for x in items if x)  # error: [implicit-bool-conversion]
    if value := count:  # error: [implicit-bool-conversion]
        pass
```

## Disabled rule

```toml
[rules]
implicit-bool-conversion = "ignore"
```

```py
def check(value: int | None):
    if value:
        pass
    result = not value
    result = value or 1
```

## Stub files

Stub bodies are not checked for implicit boolean conversions.

```pyi
def check(value: int | None):
    if value: ...
```