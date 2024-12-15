# Literal

<https://typing.readthedocs.io/en/latest/spec/literal.html#literals>

## Parameterization

```py
from typing import Literal
from enum import Enum

mode: Literal["w", "r"]
mode2: Literal["w"] | Literal["r"]
union_var: Literal[Literal[Literal[1, 2, 3], "foo"], 5, None]
a1: Literal[26]
a2: Literal[0x1A]
a3: Literal[-4]
a4: Literal["hello world"]
a5: Literal[b"hello world"]
a6: Literal[True]
a7: Literal[None]
a8: Literal[Literal[1]]
a9: Literal[Literal["w"], Literal["r"], Literal[Literal["w+"]]]

class Color(Enum):
    RED = 0
    GREEN = 1
    BLUE = 2

b1: Literal[Color.RED]

def f():
    reveal_type(mode)  # revealed: Literal["w", "r"]
    reveal_type(mode2)  # revealed: Literal["w", "r"]
    # TODO: should be revealed: Literal[1, 2, 3, "foo", 5] | None
    reveal_type(union_var)  # revealed: Literal[1, 2, 3, 5] | Literal["foo"] | None
    reveal_type(a1)  # revealed: Literal[26]
    reveal_type(a2)  # revealed: Literal[26]
    reveal_type(a3)  # revealed: Literal[-4]
    reveal_type(a4)  # revealed: Literal["hello world"]
    reveal_type(a5)  # revealed: Literal[b"hello world"]
    reveal_type(a6)  # revealed: Literal[True]
    reveal_type(a7)  # revealed: None
    reveal_type(a8)  # revealed: Literal[1]
    reveal_type(a9)  # revealed: Literal["w", "r", "w+"]
    # TODO: This should be Color.RED
    reveal_type(b1)  # revealed: Literal[0]

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

## Detecting Literal outside typing and typing_extensions

Only Literal that is defined in typing and typing_extension modules is detected as the special
Literal.

```pyi path=other.pyi
from typing import _SpecialForm

Literal: _SpecialForm
```

```py
from other import Literal

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

# error: [invalid-type-form] "`Literal` requires at least one argument when used in a type expression"
def _(x: Literal):
    reveal_type(x)  # revealed: Unknown
```
