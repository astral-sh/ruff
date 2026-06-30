# Sets

## Basic set

```py
reveal_type({1, 2})  # revealed: set[int]
reveal_type(type({1, 2}))  # revealed: <class 'set'>
reveal_type(type(set()))  # revealed: <class 'set'>
reveal_type(type(set[int]()))  # revealed: <class 'set'>
reveal_type(bool(set()))  # revealed: Literal[False]
reveal_type(bool(set[int]()))  # revealed: Literal[False]
reveal_type(bool({1}))  # revealed: Literal[True]
reveal_type(bool(set() | set()))  # revealed: Literal[False]
reveal_type(bool({1} | set()))  # revealed: Literal[True]
reveal_type(bool({1} & set()))  # revealed: Literal[False]
reveal_type(bool({1} & {2}))  # revealed: bool
reveal_type(bool(set() - {1}))  # revealed: Literal[False]
reveal_type(bool({1} - set()))  # revealed: Literal[True]
reveal_type(bool(set() ^ {1}))  # revealed: Literal[True]

def union(left: set[int], right: set[int]) -> None:
    reveal_type(type(left | right))  # revealed: type[set[int]]
    reveal_type(type({1} | right))  # revealed: type[set[int]]
```

## Set of tuples

```py
reveal_type({(1, 2), (3, 4)})  # revealed: set[tuple[int, int]]
```

## Set of functions

```py
def a(_: int) -> int:
    return 0

def b(_: int) -> int:
    return 1

x = {a, b}
reveal_type(x)  # revealed: set[(_: int) -> int]
```

## Mixed set

```py
# revealed: set[int | tuple[int, ...]]
reveal_type({1, (1, 2), (1, 2, 3)})
```

## Set comprehensions

```py
reveal_type({x for x in range(42)})  # revealed: set[int]
```
