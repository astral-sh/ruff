## Literal

```py
from typing import Literal

x: Literal[10] = 10
mode: Literal["w", "r"] = "w"
mode2: Literal["w"] | Literal["r"] = "w"

# reveal_type(Literal[26])  # revealed: Literal[26]
# reveal_type(Literal[0x1A])  # revealed: Literal[26]
# reveal_type(Literal[-4])  # revealed: Literal[-4]
# reveal_type(Literal["hello world"])  # revealed: Literal["hello world"]
# reveal_type(Literal[b"hello world"])  # revealed: Literal[b"hello world"]
# reveal_type(Literal["hello world"])  # revealed: Literal["hello world"]
# reveal_type(Literal[True])  # revealed: Literal[True]
# reveal_type(Literal[Color.RED]) # revealed: Literal["Red"]
# reveal_type(Literal[None])  # revealed: None
# TODO: "Revealed type is `Literal[1, 2, 3] | Literal["foo"] | Literal[5] | None`"
# reveal_type(Literal[Literal[Literal[1, 2, 3], "foo"], 5, None]) # revealed: Literal[1, 2, 3, "foo", 5, None]

def f():
    reveal_type(x)  # revealed: Literal[10]
    reveal_type(mode)  # revealed: Literal["w", "r"]
    # reveal_type(mode2)  # revealed: Literal["w", "r"]
```
