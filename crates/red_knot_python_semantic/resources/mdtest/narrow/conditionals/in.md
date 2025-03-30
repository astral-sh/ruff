# Narrowing for `in` conditionals

## `in` for tuples

### `in` for tuples of `int`

```py
def _(x: int):
    if x in (1, 2, 3):
        reveal_type(x)  # revealed: Literal[1, 2, 3]
    else:
        reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[2] & ~Literal[3]
```

### `in` for tuples of `object`

```py
class A: ...
class B: ...

def _(x: object):
    if x in (A(), B()):
        reveal_type(x)  # revealed: A | B
    else:
        reveal_type(x)  # revealed: ~A & ~B
```

```py
from typing import Literal

def _(x: Literal["a", "b", "c", 1]):
    if x in ("a", "b", "c"):
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: Literal[1]
```

### `in` for tuples of `str`

```py
def _(x: str):
    if x in ("a", "b", "c"):
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: str & ~Literal["a"] & ~Literal["b"] & ~Literal["c"]
```

## `in` for `str`

```py
def _(x: str):
    if x in "abc":
        # TODO: this should probably be str
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
```

```py
from typing import Literal

def _(x: Literal["a", "b", "c", "d"]):
    if x in "abc":
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: Literal["d"]
```

```py
from typing import Literal

def _(x: Literal["a", "b", "c", "e"]):
    if x in "abcd":
        reveal_type(x)  # revealed: Literal["a", "b", "c"]
    else:
        reveal_type(x)  # revealed: Literal["e"]
```
