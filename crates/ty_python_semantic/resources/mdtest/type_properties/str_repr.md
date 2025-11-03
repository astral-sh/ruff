# `__str__` and `__repr__`

```py
from typing_extensions import Literal, LiteralString
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

def _(
    a: Literal[1],
    b: Literal[True],
    c: Literal[False],
    d: Literal["ab'cd"],
    e: Literal[Answer.YES],
    f: LiteralString,
    g: int,
):
    reveal_type(str(a))  # revealed: Literal["1"]
    reveal_type(str(b))  # revealed: Literal["True"]
    reveal_type(str(c))  # revealed: Literal["False"]
    reveal_type(str(d))  # revealed: Literal["ab'cd"]
    reveal_type(str(e))  # revealed: Literal["Answer.YES"]
    reveal_type(str(f))  # revealed: LiteralString
    reveal_type(str(g))  # revealed: str

    reveal_type(repr(a))  # revealed: Literal["1"]
    reveal_type(repr(b))  # revealed: Literal["True"]
    reveal_type(repr(c))  # revealed: Literal["False"]
    reveal_type(repr(d))  # revealed: Literal["'ab\\'cd'"]
    # TODO: this could be `<Answer.YES: 1>`
    reveal_type(repr(e))  # revealed: str
    reveal_type(repr(f))  # revealed: LiteralString
    reveal_type(repr(g))  # revealed: str
```
