# Implicit boolean conversions

This opt-in rule detects truthiness checks that may accidentally conflate `None` with other falsy
values.

```toml
[rules]
implicit-bool-conversion = "warn"
```

## Basic

Consider the following problematic `take` function where a limit of `0` would be treated as if it
were `None`, which is probably not the intended behavior:

```py
def take(items: list[str], limit: int | None = None) -> list[str]:
    # snapshot: implicit-bool-conversion
    if not limit:
        return items
    return items[:limit]
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `int | None` conflates `None` with other falsy values
 --> src/mdtest_snippet.py:3:12
  |
3 |     if not limit:
  |            ^^^^^ Both `None` and `0` are falsy
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

## Type-specific explanations

```toml
[rules]
implicit-bool-conversion = "warn"
```

For common optional builtins, the annotation identifies the falsy value that can be confused with
`None`.

```py
def check(integer: int | None):
    # snapshot: implicit-bool-conversion
    if integer:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `int | None` conflates `None` with other falsy values
 --> src/mdtest_snippet.py:3:8
  |
3 |     if integer:
  |        ^^^^^^^ Both `None` and `0` are falsy
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(text: str | None):
    # snapshot: implicit-bool-conversion
    if text:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `str | None` conflates `None` with other falsy values
 --> src/mdtest_snippet.py:7:8
  |
7 |     if text:
  |        ^^^^ Both `None` and the empty string are falsy
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(data: bytes | None):
    # snapshot: implicit-bool-conversion
    if data:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `bytes | None` conflates `None` with other falsy values
  --> src/mdtest_snippet.py:11:8
   |
11 |     if data:
   |        ^^^^ Both `None` and an empty bytestring are falsy
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(number: float | None):
    # snapshot: implicit-bool-conversion
    if number:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `float | None` conflates `None` with other falsy values
  --> src/mdtest_snippet.py:15:8
   |
15 |     if number:
   |        ^^^^^^ Both `None` and `0.0` are falsy
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(flag: bool | None):
    # snapshot: implicit-bool-conversion
    if flag:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `bool | None` conflates `None` with other falsy values
  --> src/mdtest_snippet.py:19:8
   |
19 |     if flag:
   |        ^^^^ Both `None` and `False` are falsy
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(items: list[int] | None):
    # snapshot: implicit-bool-conversion
    if items:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `list[int] | None` conflates `None` with other falsy values
  --> src/mdtest_snippet.py:23:8
   |
23 |     if items:
   |        ^^^^^ Both `None` and an empty list are falsy
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(mapping: dict[str, int] | None):
    # snapshot: implicit-bool-conversion
    if mapping:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `dict[str, int] | None` conflates `None` with other falsy values
  --> src/mdtest_snippet.py:27:8
   |
27 |     if mapping:
   |        ^^^^^^^ Both `None` and an empty dictionary are falsy
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

Other optional types and unions with multiple non-`None` alternatives will use a general
explanation:

```py
def check(value: str | int | None):
    # snapshot: implicit-bool-conversion
    if value:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `str | int | None` conflates `None` with other falsy values
  --> src/mdtest_snippet.py:31:8
   |
31 |     if value:
   |        ^^^^^ Both `None` and non-`None` values can be false
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

```py
class Custom:
    def __bool__(self) -> bool:
        return False

def check(value: Custom | None):
    # snapshot: implicit-bool-conversion
    if value:
        pass
```

```snapshot
warning[implicit-bool-conversion]: Boolean test of `Custom | None` conflates `None` with other falsy values
  --> src/mdtest_snippet.py:39:8
   |
39 |     if value:
   |        ^^^^^ Both `None` and non-`None` values can be false
help: Use `is None` or `is not None` to check whether the value is present
help: Use `bool(...)` if testing truthiness is intentional
```

## Boolean contexts

The rule triggers in all of these Boolean contexts:

```py
def check(limit: int | None, items: list[int | None]):
    if limit:  # error: [implicit-bool-conversion]
        pass
    elif limit:  # error: [implicit-bool-conversion]
        pass
    while limit:  # error: [implicit-bool-conversion]
        break
    result = 1 if limit else 0  # error: [implicit-bool-conversion]
    result = not limit  # error: [implicit-bool-conversion]
    filtered = [x for x in items if x]  # error: [implicit-bool-conversion]
    match limit:
        case _ if limit:  # error: [implicit-bool-conversion]
            pass
    assert limit  # error: [implicit-bool-conversion]
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
