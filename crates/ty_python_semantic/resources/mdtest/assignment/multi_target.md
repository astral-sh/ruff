# Multi-target assignment

## Basic

```py
x = y = 1
reveal_type(x)  # revealed: Literal[1]
reveal_type(y)  # revealed: Literal[1]
```

## Assignment expressions in shared values

A value assigned to multiple targets can contain an assignment expression whose value is a lambda.
The shared assignment expression should bind its name once and give every target the same callable
type.

```py
first = second = (named := lambda: 0)

reveal_type(first)  # revealed: () -> Literal[0]
reveal_type(second)  # revealed: () -> Literal[0]
reveal_type(named)  # revealed: () -> Literal[0]
```

## Assignment expressions with unpacking targets

An unpacking target and a simple target share both the value and any assignment expressions inside
it.

```py
(first, second) = pair = ((named := 0), lambda: 1)

reveal_type(first)  # revealed: Literal[0]
reveal_type(second)  # revealed: () -> Literal[1]
reveal_type(pair)  # revealed: tuple[Literal[0], () -> Literal[1]]
reveal_type(named)  # revealed: Literal[0]
```

## Assignment expressions with subscript targets

Subscript targets infer the shared value separately from name targets, but its nested binding still
belongs to the same assignment.

```py
callbacks = [lambda: 0]
first = callbacks[0] = (named := lambda: 0)

reveal_type(first)  # revealed: () -> Literal[0]
reveal_type(named)  # revealed: () -> Literal[0]
```

## Contextual inference in shared lambdas

Each assignment target provides its own context to a shared lambda, even when the targets have
different parameter types.

```py
from collections.abc import Callable

first: Callable[[int], int]
second: Callable[[str], int]
first = second = lambda value: 0

reveal_type(first)  # revealed: (value: int) -> Literal[0]
reveal_type(second)  # revealed: (value: str) -> Literal[0]
```

## Contextual inference in assignment expressions

A declared type on the assignment-expression target still supplies context to its lambda.

```py
from collections.abc import Callable

named: Callable[[int], int]
first = second = (named := lambda value: value.bit_length())

reveal_type(first)  # revealed: (value: int) -> int
reveal_type(second)  # revealed: (value: int) -> int
reveal_type(named)  # revealed: (value: int) -> int
```

## Assignment expressions in lambda defaults

A lambda default executes in the enclosing assignment, so an assignment expression in that default
creates a binding owned by the shared assignment statement.

```py
first = second = lambda value=(named := 1): value

reveal_type(named)  # revealed: Literal[1]
```
