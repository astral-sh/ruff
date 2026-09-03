# Narrowing in `while` loops

We only make sure that narrowing works for `while` loops in general, we do not exhaustively test all
narrowing forms here, as they are covered in other tests.

Note how type narrowing works subtly different from `if` ... `else`, because the negated constraint
is retained after the loop.

## Basic `while` loop

```py
def next_item() -> int | None:
    return 1

x = next_item()

while x is not None:
    reveal_type(x)  # revealed: int
    x = next_item()

reveal_type(x)  # revealed: None
```

## `while` loop with `else`

```py
def next_item() -> int | None:
    return 1

x = next_item()

while x is not None:
    reveal_type(x)  # revealed: int
    x = next_item()
else:
    reveal_type(x)  # revealed: None

reveal_type(x)  # revealed: None
```

## Nested `while` loops

```py
from typing import Literal

def next_item() -> Literal[1, 2, 3]:
    raise NotImplementedError

x = next_item()

while x != 1:
    reveal_type(x)  # revealed: Literal[2, 3]

    while x != 2:
        reveal_type(x)  # revealed: Literal[3, 1]
        x = next_item()

    x = next_item()
```

## With `break` statements

```py
def next_item() -> int | None:
    return 1

while True:
    x = next_item()
    if x is not None:
        break

reveal_type(x)  # revealed: int
```

## Repeated attribute unwrapping

A loop can replace a wrapper with its contents and then inspect the new value. The exclusions from
earlier `isinstance` checks do not change the declared type of those contents.

```py
class Value: ...

class Required(Value):
    item: Value

class ReadOnly(Value):
    item: Value

def unwrap(value: Value) -> Value:
    while isinstance(value, (Required, ReadOnly)):
        if isinstance(value, Required):
            value = value.item
        if isinstance(value, ReadOnly):
            value = value.item
        reveal_type(value)  # revealed: Value
    reveal_type(value)  # revealed: Value & ~Required & ~ReadOnly
    return value
```

## Unwrapping union-valued attributes

When a wrapper can contain several types, narrowing applies to each possible type. Saving the
narrowed value in a tuple preserves all of those alternatives.

```py
class Value: ...

class Left(Value):
    item: Value | int | str

class Right(Value):
    item: Value | int | str

def unwrap(value: Value | int | str) -> Value | int | str:
    saved = ()
    while isinstance(value, (Left, Right)):
        if isinstance(value, Left):
            value = value.item
        if not isinstance(value, Right):
            saved = (value,)
        if isinstance(value, Right):
            value = value.item
    reveal_type(saved)  # revealed: tuple[()] | tuple[(Value & ~Right) | (int & ~Right) | (str & ~Right)]
    return value
```

## Incompatible narrowing after unwrapping

These wrappers only contain integers. After unwrapping either one, the value cannot be a string; the
other wrapper cannot be a string either because both wrapper classes are final.

```py
from typing import final

class Value: ...

@final
class Left(Value):
    item: int

@final
class Right(Value):
    item: int

def unwrap(value: Value | int) -> Value | int:
    while isinstance(value, (Left, Right)):
        if isinstance(value, Left):
            value = value.item
        if isinstance(value, str):
            reveal_type(value)  # revealed: Never
            unreachable: bytes = value
        if isinstance(value, Right):
            value = value.item
    return value
```

## Gradual narrowing after unwrapping

Excluding `list[Any]` from another `list[Any]` does not make the branch unreachable: the two gradual
types can have different materializations. The remaining wrapper is also possible in this branch.

```py
from typing import Any, final
from typing_extensions import TypeIs

class Value: ...

@final
class Left(Value):
    item: list[Any]

@final
class Right(Value):
    item: list[Any]

def is_list(value: object) -> TypeIs[list[Any]]:
    return isinstance(value, list)

def unwrap(value: Value | list[Any]) -> Value | list[Any]:
    while isinstance(value, (Left, Right)):
        if isinstance(value, Left):
            value = value.item
        if not is_list(value):
            reveal_type(value)  # revealed: Right | (list[Any] & ~list[Any])
        if isinstance(value, Right):
            value = value.item
    return value
```

## Enum narrowing after unwrapping

Excluding one enum member from an unwrapped value leaves the other member. The wrapper that has not
yet been unwrapped remains another possibility.

```py
from enum import Enum

class Value: ...

class Color(Enum):
    RED = 1
    BLUE = 2

class Left(Value):
    item: Color

class Right(Value):
    item: Color

def unwrap(value: Value | Color) -> Value | Color:
    while isinstance(value, (Left, Right)):
        if isinstance(value, Left):
            value = value.item
        if value is not Color.RED:
            reveal_type(value)  # revealed: (Right & ~Left) | Literal[Color.BLUE]
        if isinstance(value, Right):
            value = value.item
    return value
```
