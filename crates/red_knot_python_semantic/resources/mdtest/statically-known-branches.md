# Statically-known branches

## Introduction

We have the ability to infer precise types and boundness information for symbols that are defined in
branches whose conditions we can statically determine to be always true or always false. This is
useful for `sys.version_info` branches, which can make new features available based on the Python
version:

```py path=module1.py
if sys.version_info >= (3, 9):
    SomeFeature = "available"
```

If we can statically determine that the condition is always true, then we can also understand that
`SomeFeature` is always bound, without raising any errors:

```py path=test1.py
from module1 import SomeFeature

# SomeFeature is unconditionally available here, because we are on Python 3.9 or newer:
reveal_type(SomeFeature)  # revealed: Literal["available"]
```

Another scenario where this is useful is for `typing.TYPE_CHECKING` branches, which are often used
for conditional imports:

```py path=module2.py
class SomeType: ...
```

```py path=test2.py
import typing

if typing.TYPE_CHECKING:
    from module2 import SomeType

# `SomeType` is unconditionally available here for type checkers:
def f(s: SomeType) -> None: ...
```

The rest of this document contains tests for various cases where this feature can be used.

## If statements

### Always false

#### If

```py
x = 1

if False:
    x = 2

reveal_type(x)  # revealed: Literal[1]
```

#### Else

```py
x = 1

if True:
    pass
else:
    x = 2

reveal_type(x)  # revealed: Literal[1]
```

### Always true

#### If

```py
x = 1

if True:
    x = 2

reveal_type(x)  # revealed: Literal[2]
```

#### Else

```py
x = 1

if False:
    pass
else:
    x = 2

reveal_type(x)  # revealed: Literal[2]
```

### Ambiguous

Just for comparison, we still infer the combined type if the condition is not statically known:

```py
def flag() -> bool: ...

x = 1

if flag():
    x = 2

reveal_type(x)  # revealed: Literal[1, 2]
```

### Combination of always true and always false

```py
x = 1

if True:
    x = 2
else:
    x = 3

reveal_type(x)  # revealed: Literal[2]
```

### Nested conditionals

#### `if True` inside `if True`

```py
x = 1

if True:
    if True:
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2]
```

#### `if False` inside `if True`

```py
x = 1

if True:
    if False:
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[3]
```

#### `if <bool>` inside `if True`

```py
def flag() -> bool: ...

x = 1

if True:
    if flag():
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2, 3]
```

#### `if True` inside `if <bool>`

```py
def flag() -> bool: ...

x = 1

if flag():
    if True:
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2, 4]
```

#### `if True` inside `if False` ... `else`

```py
x = 1

if False:
    x = 2
else:
    if True:
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[3]
```

#### `if False` inside `if False` ... `else`

```py
x = 1

if False:
    x = 2
else:
    if False:
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[4]
```

#### `if <bool>` inside `if False` ... `else`

```py
def flag() -> bool: ...

x = 1

if False:
    x = 2
else:
    if flag():
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[3, 4]
```

### Combination with non-conditional control flow

#### `try` ... `except`

##### `if True` inside `try`

```py
def may_raise() -> None: ...

x = 1

try:
    may_raise()
    if True:
        x = 2
    else:
        x = 3
except:
    x = 4

reveal_type(x)  # revealed: Literal[2, 4]
```

##### `try` inside `if True`

```py
def may_raise() -> None: ...

x = 1

if True:
    try:
        may_raise()
        x = 2
    except KeyError:
        x = 3
    except ValueError:
        x = 4
else:
    x = 5

reveal_type(x)  # revealed: Literal[2, 3, 4]
```

##### `try` with `else` inside `if True`

```py
def may_raise() -> None: ...

x = 1

if True:
    try:
        may_raise()
        x = 2
    except KeyError:
        x = 3
    else:
        x = 4
else:
    x = 5

reveal_type(x)  # revealed: Literal[3, 4]
```

##### `try` with `finally` inside `if True`

```py
def may_raise() -> None: ...

x = 1

if True:
    try:
        may_raise()
        x = 2
    except KeyError:
        x = 3
    else:
        x = 4
    finally:
        x = 5
else:
    x = 6

reveal_type(x)  # revealed: Literal[5]
```

#### `for` loops

##### `if True` inside `for`

```py
def iterable() -> list[object]: ...

x = 1

for _ in iterable():
    if True:
        x = 2
    else:
        x = 3

reveal_type(x)  # revealed: Literal[1, 2]
```

##### `if True` inside `for` ... `else`

```py
def iterable() -> list[object]: ...

x = 1

for _ in iterable():
    x = 2
else:
    if True:
        x = 3
    else:
        x = 4

reveal_type(x)  # revealed: Literal[3]
```

##### `for` inside `if True`

```py
def iterable() -> list[object]: ...

x = 1

if True:
    for _ in iterable():
        x = 2
else:
    x = 3

reveal_type(x)  # revealed: Literal[1, 2]
```

##### `for` ... `else` inside `if True`

```py
def iterable() -> list[object]: ...

x = 1

if True:
    for _ in iterable():
        x = 2
    else:
        x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[3]
```

##### `for` loop with `break` inside `if True`

```py
def iterable() -> list[object]: ...

x = 1

if True:
    x = 2
    for _ in iterable():
        x = 3
        break
    else:
        x = 4
else:
    x = 5

reveal_type(x)  # revealed: Literal[3, 4]
```

## If expressions

See also: tests in [expression/if.md](expression/if.md).

### Always true

```py
x = 1 if True else 2

reveal_type(x)  # revealed: Literal[1]
```

### Always false

```py
x = 1 if False else 2

reveal_type(x)  # revealed: Literal[2]
```

## Boolean expressions

### Always true, `or`

```py
(x := 1) or (x := 2)

reveal_type(x)  # revealed: Literal[1]
```

### Always true, `and`

```py
(x := 1) and (x := 2)

reveal_type(x)  # revealed: Literal[2]
```

### Always false, `or`

```py
(x := 0) or (x := 2)

reveal_type(x)  # revealed: Literal[2]
```

### Always false, `and`

```py
(x := 0) and (x := 2)

reveal_type(x)  # revealed: Literal[0]
```

## While loops

### Always false

```py
x = 1

while False:
    x = 2

reveal_type(x)  # revealed: Literal[1]
```

### Always true

```py
x = 1

while True:
    x = 2
    break

reveal_type(x)  # revealed: Literal[2]
```

### Ambiguous

Make sure that we still infer the combined type if the condition is not statically known:

```py
def flag() -> bool: ...

x = 1

while flag():
    x = 2

reveal_type(x)  # revealed: Literal[1, 2]
```

### `while` ... `else`

#### `while False`

```py
while False:
    x = 1
else:
    x = 2

reveal_type(x)  # revealed: Literal[2]
```

#### `while True`

```py
while True:
    x = 1
    break
else:
    x = 2

reveal_type(x)  # revealed: Literal[1]
```

## `match` statements

### Single-valued types, always true

```py
x = 1

match "a":
    case "a":
        x = 2
    case "b":
        x = 3

reveal_type(x)  # revealed: Literal[2]
```

### Single-valued types, always false

```py
x = 1

match "something else":
    case "a":
        x = 1
    case "b":
        x = 2

reveal_type(x)  # revealed: Literal[1]
```

### Single-valued types, with wildcard pattern

This is a case that we can not handle at the moment. Our reasoning about match patterns is too
local. We can infer that the `x = 2` binding is unconditionally visible. But when we traverse all
bindings backwards, we first see the `x = 3` binding which is also visible. At the moment, we do not
mark it as *unconditionally* visible to avoid blocking off previous bindings (we would infer
`Literal[3]` otherwise).

```py
x = 1

match "a":
    case "a":
        x = 2
    case _:
        x = 3

# TODO: ideally, this should be Literal[2]
reveal_type(x)  # revealed: Literal[2, 3]
```

### Non-single-valued types

```py
def _(s: str):
    match s:
        case "a":
            x = 1
        case _:
            x = 2

    reveal_type(x)  # revealed: Literal[1, 2]
```

### `sys.version_info`

```toml
[environment]
python-version = "3.13"
```

```py
import sys

minor = "too old"

match sys.version_info.minor:
    case 12:
        minor = 12
    case 13:
        minor = 13
    case _:
        pass

reveal_type(minor)  # revealed: Literal[13]
```

## Conditional declarations

### Always false

#### `if False`

```py
x: str

if False:
    x: int

def f() -> None:
    reveal_type(x)  # revealed: str
```

#### `if True … else`

```py
x: str

if True:
    pass
else:
    x: int

def f() -> None:
    reveal_type(x)  # revealed: str
```

### Always true

#### `if True`

```py
x: str

if True:
    x: int

def f() -> None:
    reveal_type(x)  # revealed: int
```

#### `if False … else`

```py
x: str

if False:
    pass
else:
    x: int

def f() -> None:
    reveal_type(x)  # revealed: int
```

### Ambiguous

```py
def flag() -> bool: ...

x: str

if flag():
    x: int

def f() -> None:
    reveal_type(x)  # revealed: str | int
```

## Conditional function definitions

```py
def f() -> int: ...
def g() -> int: ...

if True:
    def f() -> str: ...

else:
    def g() -> str: ...

reveal_type(f())  # revealed: str
reveal_type(g())  # revealed: int
```

## Conditional class definitions

```py
if True:
    class C:
        x: int = 1

else:
    class C:
        x: str = "a"

reveal_type(C.x)  # revealed: int
```

## Conditional class attributes

```py
class C:
    if True:
        x: int = 1
    else:
        x: str = "a"

reveal_type(C.x)  # revealed: int
```

## (Un)boundness

### Unbound, `if False`

```py
if False:
    x = 1

# error: [unresolved-reference]
x
```

### Unbound, `if True … else`

```py
if True:
    pass
else:
    x = 1

# error: [unresolved-reference]
x
```

### Bound, `if True`

```py
if True:
    x = 1

# x is always bound, no error
x
```

### Bound, `if False … else`

```py
if False:
    pass
else:
    x = 1

# x is always bound, no error
x
```

### Ambiguous, possibly unbound

For comparison, we still detect definitions inside non-statically known branches as possibly
unbound:

```py
def flag() -> bool: ...

if flag():
    x = 1

# error: [possibly-unresolved-reference]
x
```

### Nested conditionals

```py
def flag() -> bool: ...

if False:
    if True:
        unbound1 = 1

if True:
    if False:
        unbound2 = 1

if False:
    if False:
        unbound3 = 1

if False:
    if flag():
        unbound4 = 1

if flag():
    if False:
        unbound5 = 1

# error: [unresolved-reference]
# error: [unresolved-reference]
# error: [unresolved-reference]
# error: [unresolved-reference]
# error: [unresolved-reference]
(unbound1, unbound2, unbound3, unbound4, unbound5)
```

### Chained conditionals

```py
if False:
    x = 1
if True:
    x = 2

# x is always bound, no error
x

if False:
    y = 1
if True:
    y = 2

# y is always bound, no error
y

if False:
    z = 1
if False:
    z = 2

# z is never bound:
# error: [unresolved-reference]
z
```

### Public boundness

```py
if True:
    x = 1

def f():
    # x is always bound, no error
    x
```

### Imports of conditionally defined symbols

#### Always false, unbound

```py path=module.py
if False:
    symbol = 1
```

```py
# error: [unresolved-import]
from module import symbol
```

#### Always true, bound

```py path=module.py
if True:
    symbol = 1
```

```py
# no error
from module import symbol
```

#### Ambiguous, possibly unbound

```py path=module.py
def flag() -> bool: ...

if flag():
    symbol = 1
```

```py
# error: [possibly-unbound-import]
from module import symbol
```

#### Always false, undeclared

```py path=module.py
if False:
    symbol: int
```

```py
# error: [unresolved-import]
from module import symbol

reveal_type(symbol)  # revealed: Unknown
```

#### Always true, declared

```py path=module.py
if True:
    symbol: int
```

```py
# no error
from module import symbol
```
