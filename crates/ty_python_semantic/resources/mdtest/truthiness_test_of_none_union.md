# Truthiness tests of unions with `None`

This opt-in rule detects truthiness checks that may accidentally conflate `None` with other falsy
values.

```toml
[rules]
truthiness-test-of-none-union = "warn"
```

## Basic

Consider the following problematic `take` function where a limit of `0` would be treated as if it
were `None`, which is probably not the intended behavior:

```py
def take(items: list[str], limit: int | None = None) -> list[str]:
    # snapshot: truthiness-test-of-none-union
    if not limit:
        return items
    return items[:limit]
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `int | None` does not distinguish `None` from other falsy values
 --> src/mdtest_snippet.py:3:12
  |
3 |     if not limit:
  |            ^^^^^ `None` and `0` are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

## Covered types

This rule triggers on unions with `None` and other types that have falsy values:

```py
from typing import Any, Literal

def check(flag: bool | None):
    if flag:  # error: [truthiness-test-of-none-union]
        pass

def check(integer: int | None):
    if integer:  # error: [truthiness-test-of-none-union]
        pass

def check(text: str | None):
    if text:  # error: [truthiness-test-of-none-union]
        pass

def check(data: bytes | None):
    if data:  # error: [truthiness-test-of-none-union]
        pass

def check(number: float | None):
    if number:  # error: [truthiness-test-of-none-union]
        pass

def check(number: complex | None):
    if number:  # error: [truthiness-test-of-none-union]
        pass

def check(items: list[int] | None):
    if items:  # error: [truthiness-test-of-none-union]
        pass

def check(items: list[Any] | None):
    if items:  # error: [truthiness-test-of-none-union]
        pass

def check(mapping: dict[str, int] | None):
    if mapping:  # error: [truthiness-test-of-none-union]
        pass
```

It also triggers if multiple other types could be falsy:

```py
def check(value: int | str | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
```

It also triggers if there are (additional) types in the union that are always truthy:

```py
def check(value: str | Literal[True] | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
```

The rule does *not* trigger on unions with `None` where the other types are always truthy, such as
`Match[str]`:

```py
import re

def re_match_is_always_truthy(match: re.Match[str]):
    reveal_type(bool(match))  # revealed: Literal[True]

def check(match: re.Match[str] | None):
    if match:
        pass
```

The rule does *not* trigger on unions with `None` that only include dynamic types. These could be
problematic in theory, but are much less likely to be a mistake:

```py
def check(value: Any | None):
    if value:
        pass
```

The rule does trigger, however, if the union includes dynamic types in addition to two types that
can be falsy:

```py
def check(value: int | Any | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
```

A custom class is also checked for implicit boolean conversion, unless it is always truthy:

```py
from typing import final

class Custom: ...

def check(value: Custom | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass

class AlwaysTruthy:
    def __bool__(self) -> Literal[True]:
        return True

def check(value: AlwaysTruthy | None):
    if value:
        pass

@final
class AlwaysTruthyFinal: ...

def check(value: AlwaysTruthyFinal | None):
    if value:
        pass
```

## Type-specific explanations

For common builtin types, the annotation identifies the falsy value that can be confused with `None`
to help users identify the problem.

Note: for a type like `int | None`, the falsy values that could be confused are not just `None` and
`0`. `False` and instances of custom subclasses of `int` could also be falsy, but it would be too
verbose to mention that in the diagnostic hint, so we just list `None` and `0` here:

```py
def check(integer: int | None):
    # snapshot: truthiness-test-of-none-union
    if integer:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `int | None` does not distinguish `None` from other falsy values
 --> src/mdtest_snippet.py:3:8
  |
3 |     if integer:
  |        ^^^^^^^ `None` and `0` are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(flag: bool | None):
    # snapshot: truthiness-test-of-none-union
    if flag:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `bool | None` does not distinguish `None` from other falsy values
 --> src/mdtest_snippet.py:7:8
  |
7 |     if flag:
  |        ^^^^ `None` and `False` are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(text: str | None):
    # snapshot: truthiness-test-of-none-union
    if text:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `str | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:11:8
   |
11 |     if text:
   |        ^^^^ `None` and the empty string are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(data: bytes | None):
    # snapshot: truthiness-test-of-none-union
    if data:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `bytes | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:15:8
   |
15 |     if data:
   |        ^^^^ `None` and an empty bytestring are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(number: float | None):
    # snapshot: truthiness-test-of-none-union
    if number:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `float | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:19:8
   |
19 |     if number:
   |        ^^^^^^ `None` and `0` are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(items: list[int] | None):
    # snapshot: truthiness-test-of-none-union
    if items:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `list[int] | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:23:8
   |
23 |     if items:
   |        ^^^^^ `None` and an empty list are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(mapping: dict[str, int] | None):
    # snapshot: truthiness-test-of-none-union
    if mapping:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `dict[str, int] | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:27:8
   |
27 |     if mapping:
   |        ^^^^^^^ `None` and an empty dictionary are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

Other optional types and unions with multiple non-`None` alternatives will use a general
explanation:

```py
def check(value: str | int | None):
    # snapshot: truthiness-test-of-none-union
    if value:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `str | int | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:31:8
   |
31 |     if value:
   |        ^^^^^ Both `None` and non-`None` values can be false
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
class Custom:
    def __bool__(self) -> bool:
        return False

def check(value: Custom | None):
    # snapshot: truthiness-test-of-none-union
    if value:
        pass
```

```snapshot
warning[truthiness-test-of-none-union]: Boolean test on `Custom | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:39:8
   |
39 |     if value:
   |        ^^^^^ Both `None` and non-`None` values can be false
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

## Boolean contexts

The rule triggers in all of these Boolean contexts:

```py
def check(limit: int | None, items: list[int | None]):
    if limit:  # error: [truthiness-test-of-none-union]
        pass
    elif limit:  # error: [truthiness-test-of-none-union]
        pass

    def _():
        while limit:  # error: [truthiness-test-of-none-union]
            break

    result = 1 if limit else 0  # error: [truthiness-test-of-none-union]

    result = not limit  # error: [truthiness-test-of-none-union]

    filtered = [x for x in items if x]  # error: [truthiness-test-of-none-union]

    match limit:
        case _ if limit:  # error: [truthiness-test-of-none-union]
            pass

    def inner():
        assert limit  # error: [truthiness-test-of-none-union]

    if result := limit:  # error: [truthiness-test-of-none-union]
        pass

    if not (result := limit):  # error: [truthiness-test-of-none-union]
        pass
```

## Boolean expressions

Boolean operators used to compute values are exempt.

```py
def check(items: list[int] | None, flag: bool, other: bool):
    value = flag and items
    value = flag or items
    value = items or []
    value = items and flag
```

When the whole expression is used as a condition, each operand is checked:

```py
    if items and flag:  # error: [truthiness-test-of-none-union]
        pass
    if flag and items:  # error: [truthiness-test-of-none-union]
        pass
    if not (items or flag):  # error: [truthiness-test-of-none-union]
        pass
    if flag and (other or items):  # error: [truthiness-test-of-none-union]
        pass
```
