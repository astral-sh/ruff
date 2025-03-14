# Binary operations on tuples

## Concatenation for heterogeneous tuples

```py
reveal_type((1, 2) + (3, 4))  # revealed: tuple[Literal[1], Literal[2], Literal[3], Literal[4]]
reveal_type(() + (1, 2))  # revealed: tuple[Literal[1], Literal[2]]
reveal_type((1, 2) + ())  # revealed: tuple[Literal[1], Literal[2]]
reveal_type(() + ())  # revealed: tuple[()]

def _(x: tuple[int, str], y: tuple[None, tuple[int]]):
    reveal_type(x + y)  # revealed: tuple[int, str, None, tuple[int]]
    reveal_type(y + x)  # revealed: tuple[None, tuple[int], int, str]
```

## Concatenation for homogeneous tuples

```py
def _(x: tuple[int, ...], y: tuple[str, ...]):
    reveal_type(x + y)  # revealed: @Todo(full tuple[...] support)
    reveal_type(x + (1, 2))  # revealed: @Todo(full tuple[...] support)
```
