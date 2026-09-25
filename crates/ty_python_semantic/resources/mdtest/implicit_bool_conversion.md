# Implicit boolean conversions

This opt-in rule requires explicit intent when testing non-boolean values for truthiness.

```toml
[environment]
python-version = "3.12"

[rules]
implicit-bool-conversion = "error"
redundant-condition = "ignore"
truthiness-test-of-callable = "ignore"
truthiness-test-of-iterable = "ignore"
```

## Missing values and zero

A valid zero must not be mistaken for a missing value. An explicit conversion is allowed when
truthiness is intentional.

```py
def process(limit: int | None):
    if not limit:  # error: [implicit-bool-conversion] "Implicit conversion of `int | None` to `bool`"
        pass
    if limit is None:
        pass
    if not bool(limit):
        pass
```

## Boolean contexts

```py
def check(items: list[int], count: int):
    if items:  # error: [implicit-bool-conversion]
        pass
    elif count:  # error: [implicit-bool-conversion]
        pass
    while items:  # error: [implicit-bool-conversion]
        break
    assert items  # error: [implicit-bool-conversion]
    result = 1 if items else 0  # error: [implicit-bool-conversion]
    result = not items  # error: [implicit-bool-conversion]
    filtered = [x for x in items if x]  # error: [implicit-bool-conversion]
    match count:
        case _ if items:  # error: [implicit-bool-conversion]
            pass
```

## Short-circuit expressions

Only operands that Python tests for truthiness need to be booleans. The last operand can be any
value when the result is used as a value. Compound conditions report the individual operands.

```py
def check(items: list[int], flag: bool, other: bool):
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
from typing import Any, Never

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

def generic[T: bool](flag: T):
    if flag:
        pass
```

## Custom truthiness

Defining `__bool__` does not exempt an object from the rule.

```py
class Custom:
    def __bool__(self) -> bool:
        return True

def check(value: Custom):
    if value:  # error: [implicit-bool-conversion]
        pass
    if bool(value):
        pass
```

## Comparisons

Comparisons usually return booleans, but a custom comparison can return another type.

```py
class Custom:
    def __lt__(self, other: "Custom") -> int:
        return 1

def check(left: Custom, right: Custom):
    if left < right:  # error: [implicit-bool-conversion]
        pass
    result = left < right
    if bool(left < right):
        pass
```

## Constants

The rule also applies to literal values, including falsy ones.

```py
if 0:  # error: [implicit-bool-conversion]
    pass
if "text":  # error: [implicit-bool-conversion]
    pass
if []:  # error: [implicit-bool-conversion]
    pass
```

## Unions and type parameters

Every possible value must be assignable to `bool`; a gradual member does not exempt a known
non-boolean member of a union.

```py
from typing import Any

def check(optional: bool | None, gradual: int | Any):
    if optional:  # error: [implicit-bool-conversion]
        pass
    if gradual:  # error: [implicit-bool-conversion]
        pass

def generic[T](value: T):
    if value:  # error: [implicit-bool-conversion]
        pass
```

## Nested scopes

```py
def check(items: list[int]):
    predicate = lambda: not items  # error: [implicit-bool-conversion]
    filtered = (x for x in items if x)  # error: [implicit-bool-conversion]
    if count := len(items):  # error: [implicit-bool-conversion]
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
