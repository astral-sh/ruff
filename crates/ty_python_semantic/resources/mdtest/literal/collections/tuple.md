# Tuples

## Empty tuple

```py
reveal_type(())  # revealed: tuple[()]
```

## Heterogeneous tuple

```py
reveal_type((1, "a"))  # revealed: tuple[Literal[1], Literal["a"]]

reveal_type((1, (2, 3)))  # revealed: tuple[Literal[1], tuple[Literal[2], Literal[3]]]

reveal_type(((1, "a"), 2))  # revealed: tuple[tuple[Literal[1], Literal["a"]], Literal[2]]
```
