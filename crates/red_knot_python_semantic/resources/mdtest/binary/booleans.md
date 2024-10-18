## Binary operations on booleans

## Basic Arithmetic

We try to be precise and all operations except for division will result in Literal type.

```py
a = True
b = False

reveal_type(a + a)  # revealed: int
reveal_type(a + b)  # revealed: int
reveal_type(b + a)  # revealed: int
reveal_type(b + b)  # revealed: int

reveal_type(a - a)  # revealed: int
reveal_type(a - b)  # revealed: int
reveal_type(b - a)  # revealed: int
reveal_type(b - b)  # revealed: int

reveal_type(a * a)  # revealed: int
reveal_type(a * b)  # revealed: int
reveal_type(b * a)  # revealed: int
reveal_type(b * b)  # revealed: int

reveal_type(a % a)  # revealed: int
reveal_type(b % a)  # revealed: int

reveal_type(a // a)  # revealed: int
reveal_type(b // a)  # revealed: int

reveal_type(a ** a)  # revealed: int
reveal_type(a ** b)  # revealed: int
reveal_type(b ** a)  # revealed: int
reveal_type(b ** b)  # revealed: int

# Division
reveal_type(a / a)  # revealed: float
reveal_type(b / a)  # revealed: float
b / b  # error: [division-by-zero] "Cannot divide object of type `Literal[False]` by zero"
a / b  # error: [division-by-zero] "Cannot divide object of type `Literal[True]` by zero"

# aitwise OR
reveal_type(a | a)  # revealed: bool
reveal_type(a | b)  # revealed: bool
reveal_type(b | a)  # revealed: bool
reveal_type(b | b)  # revealed: bool
```
