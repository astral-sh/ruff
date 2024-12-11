# Union

## Annotation

`typing.Union` can be used to construct union types same as `|` operator.

```py
from typing import Union

a: Union[int, str]
a1: Union[int, bool]
a2: Union[int, Union[float, str]]
a3: Union[int, None]
a4: Union[Union[float, str]]
a5: Union[int]
a6: Union[()]

def f():
    # revealed: int | str
    reveal_type(a)
    # Since bool is a subtype of int we simplify to int here. But we do allow assigning boolean values (see below).
    # revealed: int
    reveal_type(a1)
    # revealed: int | float | str
    reveal_type(a2)
    # revealed: int | None
    reveal_type(a3)
    # revealed: float | str
    reveal_type(a4)
    # revealed: int
    reveal_type(a5)
    # revealed: Never
    reveal_type(a6)
```

## Assignment

```py
from typing import Union

a: Union[int, str]
a = 1
a = ""
a1: Union[int, bool]
a1 = 1
a1 = True
# error: [invalid-assignment] "Object of type `Literal[b""]` is not assignable to `int | str`"
a = b""
```

## Typing Extensions

```py
from typing_extensions import Union

a: Union[int, str]

def f():
    # revealed: int | str
    reveal_type(a)
```
