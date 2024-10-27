## Literal

```py
from typing import Literal

mode: Literal["w", "r"] = "w"

# TODO: PEP-604 unions should not give error
# error: [unsupported-operator] "Operator `|` is unsupported between objects of type `object` and `object`"
mode2: Literal["w"] | Literal["r"] = "w"
# union_var: Literal[Literal[Literal[1, 2, 3], "foo"], 5, None]
# reveal_type(union_var) # revealed: Literal[1, 2, 3, "foo", 5, None]

def f():
    reveal_type(mode)  # revealed: Literal["w", "r"]
    reveal_type(mode2)  # revealed: @Todo

a: Literal[26] = 26
reveal_type(a)  # revealed: Literal[26]
a2: Literal[0x1A] = 0x1A
reveal_type(a2)  # revealed: Literal[26]
a3: Literal[-4] = -4
reveal_type(a3)  # revealed: Literal[-4]
a4: Literal["hello world"] = "hello world"
reveal_type(a4)  # revealed: Literal["hello world"]
a5: Literal[b"hello world"] = b"hello world"
reveal_type(a5)  # revealed: Literal[b"hello world"]
a6: Literal["hello world"] = "hello world"
reveal_type(a6)  # revealed: Literal["hello world"]
a7: Literal[True] = True
reveal_type(a7)  # revealed: Literal[True]
# a: Literal[Color.RED]
# reveal_type(a)
a8: Literal[None] = None
reveal_type(a8)  # revealed: None

# error: [invalid-literal-parameter] "Type arguments for `Literal` must be None, a literal value (int, bool, str, or bytes), or an enum value"
a9: Literal[3 + 4] = 7
```
