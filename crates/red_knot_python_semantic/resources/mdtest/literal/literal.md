## Literal

```py
from typing import Literal
from enum import Enum

mode: Literal["w", "r"]

mode2: Literal["w"] | Literal["r"]
union_var: Literal[Literal[Literal[1, 2, 3], "foo"], 5, None]

a: Literal[26]
a2: Literal[0x1A]
a3: Literal[-4]
a4: Literal["hello world"]
a5: Literal[b"hello world"]
a6: Literal["hello world"]
a7: Literal[True]
a8: Literal[None] = None
a9: Literal[Literal[1]]
a10: Literal[Literal["w"], Literal["r"], Literal[Literal["w+"]]]

class Color(Enum):
    RED = 0
    GREEN = 1
    BLUE = 2

a11: Literal[Color.RED]

def f():
    reveal_type(mode)  # revealed: Literal["w", "r"]
    reveal_type(mode2)  # revealed: Literal["w", "r"]
    reveal_type(a)  # revealed: Literal[26]
    reveal_type(a2)  # revealed: Literal[26]
    reveal_type(a3)  # revealed: Literal[-4]
    reveal_type(a4)  # revealed: Literal["hello world"]
    reveal_type(a5)  # revealed: Literal[b"hello world"]
    reveal_type(a6)  # revealed: Literal["hello world"]
    reveal_type(a7)  # revealed: Literal[True]
    reveal_type(a8)  # revealed: None
    reveal_type(a9)  # revealed: Literal[1]
    reveal_type(a10)  # revealed: Literal["w", "r", "w+"]
    # TODO: This should be Color.RED
    reveal_type(a11)  # revealed: Literal[0]
    # TODO: revealed: Literal[1, 2, 3, "foo", 5] | None
    reveal_type(union_var)  # revealed: Literal[1, 2, 3] | Literal["foo"] | Literal[5] | None

# error: [invalid-literal-parameter]
invalid1: Literal[3 + 4]
# error: [invalid-literal-parameter]
invalid2: Literal[4 + 3j]
# error: [invalid-literal-parameter]
invalid3: Literal[(3, 4)]
```
