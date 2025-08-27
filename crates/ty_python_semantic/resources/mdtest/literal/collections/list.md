# Lists

## Empty list

```py
reveal_type([])  # revealed: list[@Todo(list literal element type)]
```

## List comprehensions

```py
reveal_type([x for x in range(42)])  # revealed: list[@Todo(list comprehension element type)]
```
