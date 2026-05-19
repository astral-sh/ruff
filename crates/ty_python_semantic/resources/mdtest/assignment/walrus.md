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

## Shared right-hand side in multi-target assignments

A named expression on the right-hand side can be shared by multiple assignment targets.

```py
x = z = (y := 1)
reveal_type(x)  # revealed: Literal[1]
reveal_type(y)  # revealed: Literal[1]
reveal_type(z)  # revealed: Literal[1]

a: int = 0
items = [0]
a = items[0] = (b := 1)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[1]
```
