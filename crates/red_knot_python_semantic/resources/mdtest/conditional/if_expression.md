# If expressions

## Simple if-expression

```py
x = 1 if flag else 2
reveal_type(x)  # revealed: Literal[1, 2]
```

## If-expression with walrus operator

```py
y = 0
z = 0
x = (y := 1) if flag else (z := 2)
reveal_type(x)  # revealed: Literal[1, 2]
reveal_type(y)  # revealed: Literal[0, 1]
reveal_type(z)  # revealed: Literal[0, 2]
```

## Nested if-expression

```py
x = 1 if flag else 2 if flag2 else 3
reveal_type(x)  # revealed: Literal[1, 2, 3]
```

## None

```py
x = 1 if flag else None
reveal_type(x)  # revealed: Literal[1] | None
```
