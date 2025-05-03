# Narrowing with assert statements

## `assert` a value `is None` or `is not None`

```py
def _(x: str | None, y: str | None):
    assert x is not None
    reveal_type(x)  # revealed: str
    assert y is None
    reveal_type(y)  # revealed: None
```

## `assert` a value is truthy or falsy

```py
def _(x: bool, y: bool):
    assert x
    reveal_type(x)  # revealed: Literal[True]
    assert not y
    reveal_type(y)  # revealed: Literal[False]
```

## `assert` with `is` and `==` for literals

```py
from typing import Literal

def _(x: Literal[1, 2, 3], y: Literal[1, 2, 3]):
    assert x is 2
    reveal_type(x)  # revealed: Literal[2]
    assert y == 2
    reveal_type(y)  # revealed: Literal[2]
```

## `assert` with `isinstance`

```py
def _(x: int | str):
    assert isinstance(x, int)
    reveal_type(x)  # revealed: int
```

## `assert` a value `in` a tuple

```py
from typing import Literal

def _(x: Literal[1, 2, 3], y: Literal[1, 2, 3]):
    assert x in (1, 2)
    reveal_type(x)  # revealed: Literal[1, 2]
    assert y not in (1, 2)
    reveal_type(y)  # revealed: Literal[3]
```

## Assertions with messages

```py
def _(x: int | None, y: int | None):
    reveal_type(x)  # revealed: int | None
    assert x is None, reveal_type(x)  # revealed: int
    reveal_type(x)  # revealed: None

    reveal_type(y)  # revealed: int | None
    assert isinstance(y, int), reveal_type(y)  # revealed: None
    reveal_type(y)  # revealed: int
```

## Assertions with definitions inside the message

```py
def one(x: int | None):
    assert x is None, (y := x * 42) * reveal_type(y)  # revealed: int

    # error: [unresolved-reference]
    reveal_type(y)  # revealed: Unknown

def two(x: int | None, y: int | None):
    assert x is None, (y := 42) * reveal_type(y)  # revealed: Literal[42]
    reveal_type(y)  # revealed: int | None
```

## Assertions with `test` predicates that are statically known to always be `True`

```py
assert True, (x := 1)

# error: [unresolved-reference]
reveal_type(x)  # revealed: Unknown

assert False, (y := 1)

# The `assert` statement is terminal if `test` resolves to `False`,
# so even though we know the `msg` branch will have been taken here
# (we know what the truthiness of `False is!), we also know that the
# `y` definition is not visible from this point in control flow
# (because this point in control flow is unreachable).
# We make sure that this does not emit an `[unresolved-reference]`
# diagnostic by adding a reachability constraint,
# but the inferred type is `Unknown`.
#
reveal_type(y)  # revealed: Unknown
```

## Assertions with messages that reference definitions from the `test`

```py
def one(x: int | None):
    assert (y := x), reveal_type(y)  # revealed: (int & ~AlwaysTruthy) | None
    reveal_type(y)  # revealed: int & ~AlwaysFalsy

def two(x: int | None):
    assert isinstance((y := x), int), reveal_type(y)  # revealed: None
    reveal_type(y)  # revealed: int
```
