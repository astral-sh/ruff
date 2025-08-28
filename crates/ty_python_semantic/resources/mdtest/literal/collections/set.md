# Sets

## Basic set

```py
reveal_type({1, 2})  # revealed: set[@Todo(set literal element type)]
```

## Set comprehensions

```py
reveal_type({x for x in range(42)})  # revealed: set[@Todo(set comprehension element type)]
```
