# Identity tests

```py
from typing_extensions import TypeAliasType

class A: ...

def _(a1: A, a2: A, o: object):
    n1 = None
    n2 = None

    reveal_type(a1 is a1)  # revealed: bool
    reveal_type(a1 is a2)  # revealed: bool

    reveal_type(n1 is n1)  # revealed: Literal[True]
    reveal_type(n1 is n2)  # revealed: Literal[True]

    reveal_type(a1 is n1)  # revealed: Literal[False]
    reveal_type(n1 is a1)  # revealed: Literal[False]

    reveal_type(a1 is o)  # revealed: bool
    reveal_type(n1 is o)  # revealed: bool

    reveal_type(a1 is not a1)  # revealed: bool
    reveal_type(a1 is not a2)  # revealed: bool

    reveal_type(n1 is not n1)  # revealed: Literal[False]
    reveal_type(n1 is not n2)  # revealed: Literal[False]

    reveal_type(a1 is not n1)  # revealed: Literal[True]
    reveal_type(n1 is not a1)  # revealed: Literal[True]

    reveal_type(a1 is not o)  # revealed: bool
    reveal_type(n1 is not o)  # revealed: bool

def _(a1: TypeAliasType, a2: TypeAliasType):
    reveal_type(a1 is a2)  # revealed: bool
    reveal_type(a1 is not a2)  # revealed: bool

reveal_type(list[int] is list[int])  # revealed: bool
reveal_type(list[int] is not list[int])  # revealed: bool
```
