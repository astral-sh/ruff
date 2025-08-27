# PEP 613 explicit type aliases

```toml
[environment]
python-version = "3.10"
```

Explicit type aliases were introduced in PEP 613. They are defined using an annotated-assignment
statement, annotated with `typing.TypeAlias`:

## Basic

```py
from typing import TypeAlias

MyInt: TypeAlias = int

def f(x: MyInt):
    reveal_type(x)  # revealed: int

f(1)
```

## Union

For more complex type aliases, such as those involving unions or generics, the inferred value type
of the right-hand side is not a valid type for use in a type expression, and we need to infer it as
a type expression.

### Old syntax

```py
from typing import TypeAlias, Union

IntOrStr: TypeAlias = Union[int, str]

def f(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
    if isinstance(x, int):
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: str

f(1)
f("foo")
```

### New syntax

```py
from typing import TypeAlias

IntOrStr: TypeAlias = int | str

def f(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
    if isinstance(x, int):
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: str

f(1)
f("foo")
```

## Cycles

We also support cyclic type aliases:

### Old syntax

```py
from typing import Union, TypeAlias

MiniJSON: TypeAlias = Union[int, str, list["MiniJSON"]]

def f(x: MiniJSON):
    reveal_type(x)  # revealed: int | str | list[MiniJSON]
    if isinstance(x, int):
        reveal_type(x)  # revealed: int
    elif isinstance(x, str):
        reveal_type(x)  # revealed: str
    else:
        reveal_type(x)  # revealed: list[MiniJSON]

f(1)
f("foo")
f([1, "foo"])
```

### New syntax

```py
from typing import TypeAlias

MiniJSON: TypeAlias = int | str | list["MiniJSON"]

def f(x: MiniJSON):
    reveal_type(x)  # revealed: int | str | list[MiniJSON]
    if isinstance(x, int):
        reveal_type(x)  # revealed: int
    elif isinstance(x, str):
        reveal_type(x)  # revealed: str
    else:
        reveal_type(x)  # revealed: list[MiniJSON]

f(1)
f("foo")
f([1, "foo"])
```

### Union inside generic

#### With old-style union

```py
from typing import Union, TypeAlias

A: TypeAlias = list[Union["A", str]]

def f(x: A):
    reveal_type(x)  # revealed: list[A | str]
    for item in x:
        reveal_type(item)  # revealed: list[A | str] | str
```

#### With new-style union

```py
from typing import TypeAlias

A: TypeAlias = list["A" | str]

def f(x: A):
    reveal_type(x)  # revealed: list[A | str]
    for item in x:
        reveal_type(item)  # revealed: list[A | str] | str
```

#### With Optional

```py
from typing import Optional, Union, TypeAlias

A: TypeAlias = list[Optional[Union["A", str]]]

def f(x: A):
    reveal_type(x)  # revealed: list[A | str | None]
    for item in x:
        reveal_type(item)  # revealed: list[A | str | None] | str | None
```

### Invalid examples

#### No value

```py
from typing import TypeAlias

# TODO: error
Bad: TypeAlias

# Nested function so we don't emit unresolved-reference for `Bad`:
def _():
    def f(x: Bad):
        reveal_type(x)  # revealed: Unknown
```

#### No value, in stub

`stub.pyi`:

```pyi
from typing import TypeAlias

# TODO: error
Bad: TypeAlias
```

`main.py`:

```py
from stub import Bad

def f(x: Bad):
    reveal_type(x)  # revealed: Unknown
```
