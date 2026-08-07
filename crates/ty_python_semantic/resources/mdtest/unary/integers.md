# Unary Operations

## Unary Addition

```py
reveal_type(+0)  # revealed: Literal[0]
reveal_type(+1)  # revealed: Literal[1]
reveal_type(+True)  # revealed: Literal[1]
```

## Unary Subtraction

```py
reveal_type(-0)  # revealed: Literal[0]
reveal_type(-1)  # revealed: Literal[-1]
reveal_type(-True)  # revealed: Literal[-1]
```

## Unary Bitwise Inversion

```py
reveal_type(~0)  # revealed: Literal[-1]
reveal_type(~1)  # revealed: Literal[-2]

# `~` on a `bool` is deprecated and will be removed in Python 3.16.
# error: [deprecated]
# revealed: Literal[-2]
reveal_type(~True)
```
