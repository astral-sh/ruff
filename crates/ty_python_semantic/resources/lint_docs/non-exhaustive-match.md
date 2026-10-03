## What it does

Detects `match` statements that may not cover every possible subject value.

## Why is this bad?

If no case matches, none of the case bodies runs, and execution continues after the `match`
statement. Missing a case can therefore leave a value unhandled, especially when a new member is
added to an enum or a union.

## Rule status

This rule is disabled by default. Exhaustive matches can make code more robust, but requiring every
match to be exhaustive is an opinionated choice: a non-exhaustive match can be intentional and is
not necessarily incorrect.

## Example

```py
from enum import Enum


class Direction(Enum):
    NORTH = 1
    SOUTH = 2


def describe(direction: Direction) -> None:
    match direction:  # error: [non-exhaustive-match]
        case Direction.NORTH:
            print("north")
```

Handle the missing case:

```py
def describe_complete(direction: Direction) -> None:
    match direction:
        case Direction.NORTH:
            print("north")
        case Direction.SOUTH:
            print("south")
```

If the other values are intentionally ignored, use a wildcard case to make that explicit:

```py
def describe_north(direction: Direction) -> None:
    match direction:
        case Direction.NORTH:
            print("north")
        case _:
            pass
```
