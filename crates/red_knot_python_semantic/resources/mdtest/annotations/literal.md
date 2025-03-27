# Literal

<https://typing.readthedocs.io/en/latest/spec/literal.html#literals>

## Parameterization

```py
from typing import Literal
from enum import Enum

mode: Literal["w", "r"]
a1: Literal[26]
a2: Literal[0x1A]
a3: Literal[-4]
a4: Literal["hello world"]
a5: Literal[b"hello world"]
a6: Literal[True]
a7: Literal[None]
a8: Literal[Literal[1]]

class Color(Enum):
    RED = 0
    GREEN = 1
    BLUE = 2

b1: Literal[Color.RED]

def f():
    reveal_type(mode)  # revealed: Literal["w", "r"]
    reveal_type(a1)  # revealed: Literal[26]
    reveal_type(a2)  # revealed: Literal[26]
    reveal_type(a3)  # revealed: Literal[-4]
    reveal_type(a4)  # revealed: Literal["hello world"]
    reveal_type(a5)  # revealed: Literal[b"hello world"]
    reveal_type(a6)  # revealed: Literal[True]
    reveal_type(a7)  # revealed: None
    reveal_type(a8)  # revealed: Literal[1]
    # TODO: This should be Color.RED
    reveal_type(b1)  # revealed: @Todo(Attribute access on enum classes)

# error: [invalid-type-form]
invalid1: Literal[3 + 4]
# error: [invalid-type-form]
invalid2: Literal[4 + 3j]
# error: [invalid-type-form]
invalid3: Literal[(3, 4)]

hello = "hello"
invalid4: Literal[
    1 + 2,  # error: [invalid-type-form]
    "foo",
    hello,  # error: [invalid-type-form]
    (1, 2, 3),  # error: [invalid-type-form]
]
```

## Shortening unions of literals

When a Literal is parameterized with more than one value, it’s treated as exactly to equivalent to
the union of those types.

```py
from typing import Literal

def x(
    a1: Literal[Literal[Literal[1, 2, 3], "foo"], 5, None],
    a2: Literal["w"] | Literal["r"],
    a3: Literal[Literal["w"], Literal["r"], Literal[Literal["w+"]]],
    a4: Literal[True] | Literal[1, 2] | Literal["foo"],
):
    reveal_type(a1)  # revealed: Literal[1, 2, 3, "foo", 5] | None
    reveal_type(a2)  # revealed: Literal["w", "r"]
    reveal_type(a3)  # revealed: Literal["w", "r", "w+"]
    reveal_type(a4)  # revealed: Literal[True, 1, 2, "foo"]
```

## Display of heterogeneous unions of literals

```py
from typing import Literal, Union

def foo(x: int) -> int:
    return x + 1

def bar(s: str) -> str:
    return s

class A: ...
class B: ...

def union_example(
    x: Union[
        # unknown type
        # error: [unresolved-reference]
        y,
        Literal[-1],
        Literal["A"],
        Literal[b"A"],
        Literal[b"\x00"],
        Literal[b"\x07"],
        Literal[0],
        Literal[1],
        Literal["B"],
        Literal["foo"],
        Literal["bar"],
        Literal["B"],
        Literal[True],
        None,
    ],
):
    reveal_type(x)  # revealed: Unknown | Literal[-1, "A", b"A", b"\x00", b"\x07", 0, 1, "B", "foo", "bar", True] | None
```

## Detecting Literal outside typing and typing_extensions

Only Literal that is defined in typing and typing_extension modules is detected as the special
Literal.

`other.pyi`:

```pyi
from typing import _SpecialForm

Literal: _SpecialForm
```

```py
from other import Literal

# TODO: can we add a subdiagnostic here saying something like:
#
#     `other.Literal` and `typing.Literal` have similar names, but are different symbols and don't have the same semantics
#
# ?
#
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
a1: Literal[26]

def f():
    reveal_type(a1)  # revealed: @Todo(generics)
```

## Detecting typing_extensions.Literal

```py
from typing_extensions import Literal

a1: Literal[26]

def f():
    reveal_type(a1)  # revealed: Literal[26]
```

## Invalid

```py
from typing import Literal

# error: [invalid-type-form] "`typing.Literal` requires at least one argument when used in a type expression"
def _(x: Literal):
    reveal_type(x)  # revealed: Unknown
```
