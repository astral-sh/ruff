# Statically-known branches

## Introduction

We have the ability to infer precise types and boundness information for symbols that are defined in
branches whose conditions we can statically determine to be always true or always false. This is
useful for `sys.version_info` branches, which can make new features available based on the Python
version:

If we can statically determine that the condition is always true, then we can also understand that
`SomeFeature` is always bound, without raising any errors:

```py
import sys

class C:
    if sys.version_info >= (3, 9):
        SomeFeature: str = "available"

# C.SomeFeature is unconditionally available here, because we are on Python 3.9 or newer:
reveal_type(C.SomeFeature)  # revealed: str
```

Another scenario where this is useful is for `typing.TYPE_CHECKING` branches, which are often used
for conditional imports:

`module.py`:

```py
class SomeType: ...
```

`main.py`:

```py
import typing

if typing.TYPE_CHECKING:
    from module import SomeType

# `SomeType` is unconditionally available here for type checkers:
def f(s: SomeType) -> None: ...
```

## Common use cases

This section makes sure that we can handle all commonly encountered patterns of static conditions.

### `sys.version_info`

```toml
[environment]
python-version = "3.10"
```

```py
import sys

if sys.version_info >= (3, 11):
    greater_equals_311 = True
elif sys.version_info >= (3, 9):
    greater_equals_309 = True
else:
    less_than_309 = True

if sys.version_info[0] == 2:
    python2 = True

# error: [unresolved-reference]
greater_equals_311

# no error
greater_equals_309

# error: [unresolved-reference]
less_than_309

# error: [unresolved-reference]
python2
```

### `sys.platform`

```toml
[environment]
python-platform = "linux"
```

```py
import sys

if sys.platform == "linux":
    linux = True
elif sys.platform == "darwin":
    darwin = True
else:
    other = True

# no error
linux

# error: [unresolved-reference]
darwin

# error: [unresolved-reference]
other
```

### `typing.TYPE_CHECKING`

```py
import typing

if typing.TYPE_CHECKING:
    type_checking = True
else:
    runtime = True

# no error
type_checking

# error: [unresolved-reference]
runtime
```

### Combination of `sys.platform` check and `sys.version_info` check

```toml
[environment]
python-version = "3.10"
python-platform = "darwin"
```

```py
import sys

if sys.platform == "darwin" and sys.version_info >= (3, 11):
    only_platform_check_true = True
elif sys.platform == "win32" and sys.version_info >= (3, 10):
    only_version_check_true = True
elif sys.platform == "linux" and sys.version_info >= (3, 11):
    both_checks_false = True
elif sys.platform == "darwin" and sys.version_info >= (3, 10):
    both_checks_true = True
else:
    other = True

# error: [unresolved-reference]
only_platform_check_true

# error: [unresolved-reference]
only_version_check_true

# error: [unresolved-reference]
both_checks_false

# no error
both_checks_true

# error: [unresolved-reference]
other
```

## Based on type inference

For the the rest of this test suite, we will mostly use `True` and `False` literals to indicate
statically known conditions, but here, we show that the results are truly based on type inference,
not some special handling of specific conditions in semantic index building. We use two modules to
demonstrate this, since semantic index building is inherently single-module:

`module.py`:

```py
from typing import Literal

class AlwaysTrue:
    def __bool__(self) -> Literal[True]:
        return True
```

```py
from module import AlwaysTrue

if AlwaysTrue():
    yes = True
else:
    no = True

# no error
yes

# error: [unresolved-reference]
no
```

## If statements

The rest of this document contains tests for various control flow elements. This section tests `if`
statements.

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
def flag() -> bool:
    return True

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

### `elif` branches

#### Always false

```py
def flag() -> bool:
    return True

x = 1

if flag():
    x = 2
elif False:
    x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2, 4]
```

#### Always true

```py
def flag() -> bool:
    return True

x = 1

if flag():
    x = 2
elif True:
    x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2, 3]
```

#### Ambiguous

```py
def flag() -> bool:
    return True

x = 1

if flag():
    x = 2
elif flag():
    x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[2, 3, 4]
```

#### Multiple `elif` branches, always false

Make sure that we include bindings from all non-`False` branches:

```py
def flag() -> bool:
    return True

x = 1

if flag():
    x = 2
elif flag():
    x = 3
elif False:
    x = 4
elif False:
    x = 5
elif flag():
    x = 6
elif flag():
    x = 7
else:
    x = 8

reveal_type(x)  # revealed: Literal[2, 3, 6, 7, 8]
```

#### Multiple `elif` branches, always true

Make sure that we only include the binding from the first `elif True` branch:

```py
def flag() -> bool:
    return True

x = 1

if flag():
    x = 2
elif flag():
    x = 3
elif True:
    x = 4
elif True:
    x = 5
elif flag():
    x = 6
else:
    x = 7

reveal_type(x)  # revealed: Literal[2, 3, 4]
```

#### `elif` without `else` branch, always true

```py
def flag() -> bool:
    return True

x = 1

if flag():
    x = 2
elif True:
    x = 3

reveal_type(x)  # revealed: Literal[2, 3]
```

#### `elif` without `else` branch, always false

```py
def flag() -> bool:
    return True

x = 1

if flag():
    x = 2
elif False:
    x = 3

reveal_type(x)  # revealed: Literal[1, 2]
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

reveal_type(x)  # revealed: Literal[1]
```

#### `if <bool>` inside `if True`

```py
def flag() -> bool:
    return True

x = 1

if True:
    if flag():
        x = 2
else:
    x = 3

reveal_type(x)  # revealed: Literal[1, 2]
```

#### `if True` inside `if <bool>`

```py
def flag() -> bool:
    return True

x = 1

if flag():
    if True:
        x = 2
else:
    x = 3

reveal_type(x)  # revealed: Literal[2, 3]
```

#### `if True` inside `if False` ... `else`

```py
x = 1

if False:
    x = 2
else:
    if True:
        x = 3

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

reveal_type(x)  # revealed: Literal[1]
```

#### `if <bool>` inside `if False` ... `else`

```py
def flag() -> bool:
    return True

x = 1

if False:
    x = 2
else:
    if flag():
        x = 3

reveal_type(x)  # revealed: Literal[1, 3]
```

### Nested conditionals (with inner `else`)

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
def flag() -> bool:
    return True

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
def flag() -> bool:
    return True

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
def flag() -> bool:
    return True

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
def iterable() -> list[object]:
    return [1, ""]

x = 1

for _ in iterable():
    x = 2
    if True:
        x = 3

reveal_type(x)  # revealed: Literal[1, 3]
```

##### `if True` inside `for` ... `else`

```py
def iterable() -> list[object]:
    return [1, ""]

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
def iterable() -> list[object]:
    return [1, ""]

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
def iterable() -> list[object]:
    return [1, ""]

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
def iterable() -> list[object]:
    return [1, ""]

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

Note that the result type of an `if`-expression can be precisely inferred if the condition is
statically known. This is a plain type inference feature that does not need support for statically
known branches. The tests for this feature are in [expression/if.md](expression/if.md).

The tests here make sure that we also handle assignment expressions inside `if`-expressions
correctly.

### Type inference

### Always true

```py
x = (y := 1) if True else (y := 2)

reveal_type(x)  # revealed: Literal[1]
reveal_type(y)  # revealed: Literal[1]
```

### Always false

```py
x = (y := 1) if False else (y := 2)

reveal_type(x)  # revealed: Literal[2]
reveal_type(y)  # revealed: Literal[2]
```

## Boolean expressions

### Always true, `or`

```py
(x := 1) or (x := 2)

reveal_type(x)  # revealed: Literal[1]

(y := 1) or (y := 2) or (y := 3) or (y := 4)

reveal_type(y)  # revealed: Literal[1]
```

### Always true, `and`

```py
(x := 1) and (x := 2)

reveal_type(x)  # revealed: Literal[2]

(y := 1) and (y := 2) and (y := 3) and (y := 4)

reveal_type(y)  # revealed: Literal[4]
```

### Always false, `or`

```py
(x := 0) or (x := 1)

reveal_type(x)  # revealed: Literal[1]

(y := 0) or (y := 0) or (y := 1) or (y := 2)

reveal_type(y)  # revealed: Literal[1]
```

### Always false, `and`

```py
(x := 0) and (x := 1)

reveal_type(x)  # revealed: Literal[0]

(y := 0) and (y := 1) and (y := 2) and (y := 3)

reveal_type(y)  # revealed: Literal[0]
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
def flag() -> bool:
    return True

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

#### `while False` with `break`

```py
x = 1
while False:
    x = 2
    break
    x = 3
else:
    x = 4

reveal_type(x)  # revealed: Literal[4]
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

### Single-valued types, always true, with wildcard pattern

```py
x = 1

match "a":
    case "a":
        x = 2
    case "b":
        x = 3
    case _:
        pass

reveal_type(x)  # revealed: Literal[2]
```

### Single-valued types, always true, with guard

Make sure we don't infer a static truthiness in case there is a case guard:

```py
def flag() -> bool:
    return True

x = 1

match "a":
    case "a" if flag():
        x = 2
    case "b":
        x = 3
    case _:
        pass

reveal_type(x)  # revealed: Literal[1, 2]
```

### Single-valued types, always false

```py
x = 1

match "something else":
    case "a":
        x = 2
    case "b":
        x = 3

reveal_type(x)  # revealed: Literal[1]
```

### Single-valued types, always false, with wildcard pattern

```py
x = 1

match "something else":
    case "a":
        x = 2
    case "b":
        x = 3
    case _:
        pass

reveal_type(x)  # revealed: Literal[1]
```

### Single-valued types, always false, with guard

For definitely-false cases, the presence of a guard has no influence:

```py
def flag() -> bool:
    return True

x = 1

match "something else":
    case "a" if flag():
        x = 2
    case "b":
        x = 3
    case _:
        pass

reveal_type(x)  # revealed: Literal[1]
```

### Non-single-valued types

```py
def _(s: str):
    match s:
        case "a":
            x = 1
        case "b":
            x = 2
        case _:
            x = 3

    reveal_type(x)  # revealed: Literal[1, 2, 3]
```

### Matching on `sys.platform`

```toml
[environment]
python-platform = "darwin"
```

```py
import sys

match sys.platform:
    case "linux":
        linux = True
    case "darwin":
        darwin = True
    case "win32":
        win32 = True
    case _:
        other = True

# error: [unresolved-reference]
linux

# no error
darwin

# error: [unresolved-reference]
win32

# error: [unresolved-reference]
other
```

### Matching on `sys.version_info`

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
def flag() -> bool:
    return True

x: str

if flag():
    x: int

def f() -> None:
    reveal_type(x)  # revealed: str | int
```

## Conditional function definitions

```py
def f() -> int:
    return 1

def g() -> int:
    return 1

if True:
    def f() -> str:
        return ""

else:
    def g() -> str:
        return ""

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
def flag() -> bool:
    return True

if flag():
    x = 1

# error: [possibly-unresolved-reference]
x
```

### Nested conditionals

```py
def flag() -> bool:
    return True

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

`module.py`:

```py
if False:
    symbol = 1
```

```py
# error: [unresolved-import]
from module import symbol
```

#### Always true, bound

`module.py`:

```py
if True:
    symbol = 1
```

```py
# no error
from module import symbol
```

#### Ambiguous, possibly unbound

`module.py`:

```py
def flag() -> bool:
    return True

if flag():
    symbol = 1
```

```py
# error: [possibly-unbound-import]
from module import symbol
```

#### Always false, undeclared

`module.py`:

```py
if False:
    symbol: int
```

```py
# error: [unresolved-import]
from module import symbol

reveal_type(symbol)  # revealed: Unknown
```

#### Always true, declared

`module.py`:

```py
if True:
    symbol: int
```

```py
# no error
from module import symbol
```

## Unsupported features

We do not support full unreachable code analysis yet. We also raise diagnostics from
statically-known to be false branches:

```py
if False:
    # error: [unresolved-reference]
    x
```
