# Errors while declaring

## Violates previous assignment

```py
x = 1
x: str  # error: [invalid-declaration] "Cannot declare type `str` for inferred type `Literal[1]`"
```

## Declarations in while loops provide assignment context

A declaration inside a loop also applies to assignments that reach the declaration from the previous
iteration.

```py
from typing import TypedDict

class Record(TypedDict):
    value: int

while True:
    record: Record
    record = {"value": 1}
    reveal_type(record)  # revealed: Record
```

## Declarations in for loops provide assignment context

The same declaration context applies when assignments reach a declaration from an earlier `for` loop
iteration.

```py
from typing import TypedDict

class Record(TypedDict):
    value: int

for _ in range(2):
    record: Record
    record = {"value": 1}
    reveal_type(record)  # revealed: Record
```

## Declarations in loops provide nested assignment context

A declaration inside a loop also provides context for values nested within a dictionary literal.

```py
from typing import TypedDict

class Record(TypedDict):
    values: list[float]

while True:
    record: Record
    record = {"values": [1]}
    reveal_type(record)  # revealed: Record
```

## Declarations in loops reject invalid assignments

An incompatible dictionary item is still reported at the assignment, not at the declaration.

```py
from typing import TypedDict

class Record(TypedDict):
    value: int

while True:
    record: Record
    record = {"value": "invalid"}  # error: [invalid-argument-type]
    reveal_type(record)  # revealed: Record
```

## Declarations in loops reject incompatible earlier bindings

An incompatible binding that predates the loop must still invalidate a declaration inside it.

```py
from typing import TypedDict

class Record(TypedDict):
    value: int

record = {"value": 1}

while True:
    record: Record  # error: [invalid-declaration]
    record = {"value": 1}
```

## Stringified declarations in loops provide assignment context

String annotations still provide their resolved type when a loop-carried assignment needs context.

```py
from typing import TypedDict

class Record(TypedDict):
    value: int

while True:
    record: "Record"
    record = {"value": 1}
    reveal_type(record)  # revealed: Record
```

## Incompatible declarations

```py
def _(flag: bool):
    if flag:
        x: str
    else:
        x: int

    x = 1  # error: [conflicting-declarations] "Conflicting declared types for `x`: `str` and `int`"
```

## Incompatible declarations for 2 (out of 3) types

```py
def _(flag1: bool, flag2: bool):
    if flag1:
        x: str
    elif flag2:
        x: int

    # Here, the declared type for `x` is `int | str | Unknown`.
    x = 1  # error: [conflicting-declarations] "Conflicting declared types for `x`: `str` and `int`"
```

## Incompatible declarations with repeated types

```py
def _(flag1: bool, flag2: bool, flag3: bool, flag4: bool):
    if flag1:
        x: str
    elif flag2:
        x: int
    elif flag3:
        x: int
    elif flag4:
        x: str
    else:
        x: bytes

    x = "a"  # error: [conflicting-declarations] "Conflicting declared types for `x`: `str`, `int` and `bytes`"
```

## Incompatible declarations with bad assignment

```py
def _(flag: bool):
    if flag:
        x: str
    else:
        x: int

    # error: [conflicting-declarations]
    # error: [invalid-assignment]
    x = b"foo"
```

## No errors

Currently, we avoid raising the conflicting-declarations for the following cases:

### Partial declarations

```py
def _(flag: bool):
    if flag:
        x: int

    x = 1
```

### Partial declarations in try-except

Refer to <https://github.com/astral-sh/ruff/issues/13966>

```py
def _():
    try:
        x: int = 1
    except:
        x = 2

    x = 3
```
