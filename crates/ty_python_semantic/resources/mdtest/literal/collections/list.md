# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
```

## Heterogeneous list

```py
reveal_type([1, "a"])  # revealed: list[int | str]

reveal_type([1, (2, 3)])  # revealed: list[int | tuple[int, int]]

reveal_type([[1, "a", 2]])  # revealed: list[list[int | str]]
```
