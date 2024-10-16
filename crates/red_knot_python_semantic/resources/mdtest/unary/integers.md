# Unary Operations

## Unary Addition

```py
a = +0
b = +1
c = +True

reveal_type(a)  # revealed: Literal[0]
reveal_type(b)  # revealed: Literal[1]
reveal_type(c)  # revealed: Literal[1]
```

## Unary Subtraction

```py
a = -0
b = -1
c = -True

reveal_type(a)  # revealed: Literal[0]
reveal_type(b)  # revealed: Literal[-1]
reveal_type(c)  # revealed: Literal[-1]
```

## Unary Bitwise Inversion

```py
a = ~0
b = ~1
c = ~True

reveal_type(a) # revealed: Literal[-1] 
reveal_type(b) # revealed: Literal[-2]
reveal_type(c) # revealed: Literal[-2]
```
