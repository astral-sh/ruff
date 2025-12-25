# Walrus operator

## Basic

```py
x = (y := 1) + 1
reveal_type(x)  # revealed: Literal[2]
reveal_type(y)  # revealed: Literal[1]
```

## Walrus self-addition

```py
x = 0
(x := x + 1)
reveal_type(x)  # revealed: Literal[1]
```
