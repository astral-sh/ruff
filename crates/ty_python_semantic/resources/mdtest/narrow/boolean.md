# Narrowing in boolean expressions

In `or` expressions, the right-hand side is evaluated only if the left-hand side is **falsy**. So
when the right-hand side is evaluated, we know the left side has failed.

Similarly, in `and` expressions, the right-hand side is evaluated only if the left-hand side is
**truthy**. So when the right-hand side is evaluated, we know the left side has succeeded.

## Narrowing in `or`

```py
class A: ...

def _(x: A | None):
    isinstance(x, A) or reveal_type(x)  # revealed: None
    x is None or reveal_type(x)  # revealed: A
    reveal_type(x)  # revealed: A | None
```

## Narrowing in `and`

```py
from typing import final

class A: ...

def _(x: A | None):
    isinstance(x, A) and reveal_type(x)  # revealed: A
    x is None and reveal_type(x)  # revealed: None
    reveal_type(x)  # revealed: A | None

@final
class FinalClass: ...

# We know that no subclass of `FinalClass` can exist,
# therefore no subtype of `FinalClass` can define `__bool__`
# or `__len__`, therefore `FinalClass` can safely be considered
# always-truthy, therefore this always resolves to `None`
reveal_type(FinalClass() and None)  # revealed: None
```

## Multiple `and` arms

```py
class A: ...

def _(x: A | None, flag1: bool, flag2: bool):
    flag1 and isinstance(x, A) and reveal_type(x)  # revealed: A
    isinstance(x, A) and flag1 and reveal_type(x)  # revealed: A
    reveal_type(x) and isinstance(x, A) and flag2  # revealed: A | None
```

## Multiple `or` arms

```py
class A: ...

def _(x: A | None, flag1: bool, flag2: bool, flag3: bool):
    flag1 or isinstance(x, A) or reveal_type(x)  # revealed: None
    isinstance(x, A) or flag2 or reveal_type(x)  # revealed: None
    reveal_type(x) or isinstance(x, A) or flag3  # revealed: A | None
```

## Multiple predicates

```py
from typing import Literal

class A: ...

def _(x: A | None | Literal[1]):
    x is None or isinstance(x, A) or reveal_type(x)  # revealed: Literal[1]
```

## Mix of `and` and `or`

```py
from typing import Literal

class A: ...

def _(x: A | None | Literal[1]):
    isinstance(x, A) or x is not None and reveal_type(x)  # revealed: Literal[1]
```
