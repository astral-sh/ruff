# Dictionaries

## Empty dictionary

```py
reveal_type({})  # revealed: dict[@Todo(dict literal key type), @Todo(dict literal value type)]
```

## Dict comprehensions

```py
# revealed: dict[@Todo(dict comprehension key type), @Todo(dict comprehension value type)]
reveal_type({x: y for x, y in enumerate(range(42))})
```
