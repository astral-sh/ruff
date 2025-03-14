# Narrowing in boolean expressions

In `or` expressions, the right-hand side is evaluated only if the left-hand side is **falsy**. So
when the right-hand side is evaluated, we know the left side has failed.

Similarly, in `and` expressions, the right-hand side is evaluated only if the left-hand side is
**truthy**. So when the right-hand side is evaluated, we know the left side has succeeded.

## Narrowing in `or`

```py
def _(flag: bool):
    class A: ...
    x: A | None = A() if flag else None

    isinstance(x, A) or reveal_type(x)  # revealed: None
    x is None or reveal_type(x)  # revealed: A
    reveal_type(x)  # revealed: A | None
```

## Narrowing in `and`

```py
def _(flag: bool):
    class A: ...
    x: A | None = A() if flag else None

    isinstance(x, A) and reveal_type(x)  # revealed: A
    x is None and reveal_type(x)  # revealed: None
    reveal_type(x)  # revealed: A | None
```

## Multiple `and` arms

```py
def _(flag1: bool, flag2: bool, flag3: bool, flag4: bool):
    class A: ...
    x: A | None = A() if flag1 else None

    flag2 and isinstance(x, A) and reveal_type(x)  # revealed: A
    isinstance(x, A) and flag2 and reveal_type(x)  # revealed: A
    reveal_type(x) and isinstance(x, A) and flag3  # revealed: A | None
```

## Multiple `or` arms

```py
def _(flag1: bool, flag2: bool, flag3: bool, flag4: bool):
    class A: ...
    x: A | None = A() if flag1 else None

    flag2 or isinstance(x, A) or reveal_type(x)  # revealed: None
    isinstance(x, A) or flag3 or reveal_type(x)  # revealed: None
    reveal_type(x) or isinstance(x, A) or flag4  # revealed: A | None
```

## Multiple predicates

```py
from typing import Literal

def _(flag1: bool, flag2: bool):
    class A: ...
    x: A | None | Literal[1] = A() if flag1 else None if flag2 else 1

    x is None or isinstance(x, A) or reveal_type(x)  # revealed: Literal[1]
```

## Mix of `and` and `or`

```py
from typing import Literal

def _(flag1: bool, flag2: bool):
    class A: ...
    x: A | None | Literal[1] = A() if flag1 else None if flag2 else 1

    isinstance(x, A) or x is not None and reveal_type(x)  # revealed: Literal[1]
```
