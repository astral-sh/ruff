# Optional

## Annotation

`typing.Optional` is equivalent to using the type with a None in a Union.

```py
from typing import Optional

a: Optional[int]
a1: Optional[bool]
a2: Optional[Optional[bool]]
a3: Optional[None]

def f():
    # revealed: int | None
    reveal_type(a)
    # revealed: bool | None
    reveal_type(a1)
    # revealed: bool | None
    reveal_type(a2)
    # revealed: None
    reveal_type(a3)
```

## Assignment

```py
from typing import Optional

a: Optional[int] = 1
a = None
# error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int | None`"
a = ""
```

## Typing Extensions

```py
from typing_extensions import Optional

a: Optional[int]

def f():
    # revealed: int | None
    reveal_type(a)
```

## Invalid

```py
from typing import Optional

# error: [invalid-type-form] "`typing.Optional` requires exactly one argument when used in a type expression"
def f(x: Optional) -> None:
    reveal_type(x)  # revealed: Unknown
```
