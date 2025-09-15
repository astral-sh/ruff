# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
```

## List of tuples

```py
reveal_type([(1, 2), (3, 4)])  # revealed: list[Unknown | tuple[int, int]]
```

## Mixed list

```py
# revealed: list[Unknown | int | tuple[int, int] | tuple[int, int, int]]
reveal_type([1, (1, 2), (1, 2, 3)])
```

## List comprehensions

```py
reveal_type([x for x in range(42)])  # revealed: list[@Todo(list comprehension element type)]
```
