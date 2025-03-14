## Binary operations on booleans

## Basic Arithmetic

We try to be precise and all operations except for division will result in Literal type.

```py
a = True
b = False

reveal_type(a + a)  # revealed: Literal[2]
reveal_type(a + b)  # revealed: Literal[1]
reveal_type(b + a)  # revealed: Literal[1]
reveal_type(b + b)  # revealed: Literal[0]

reveal_type(a - a)  # revealed: Literal[0]
reveal_type(a - b)  # revealed: Literal[1]
reveal_type(b - a)  # revealed: Literal[-1]
reveal_type(b - b)  # revealed: Literal[0]

reveal_type(a * a)  # revealed: Literal[1]
reveal_type(a * b)  # revealed: Literal[0]
reveal_type(b * a)  # revealed: Literal[0]
reveal_type(b * b)  # revealed: Literal[0]

reveal_type(a % a)  # revealed: Literal[0]
reveal_type(b % a)  # revealed: Literal[0]

reveal_type(a // a)  # revealed: Literal[1]
reveal_type(b // a)  # revealed: Literal[0]

reveal_type(a**a)  # revealed: Literal[1]
reveal_type(a**b)  # revealed: Literal[1]
reveal_type(b**a)  # revealed: Literal[0]
reveal_type(b**b)  # revealed: Literal[1]

# Division
reveal_type(a / a)  # revealed: float
reveal_type(b / a)  # revealed: float
b / b  # error: [division-by-zero] "Cannot divide object of type `Literal[False]` by zero"
a / b  # error: [division-by-zero] "Cannot divide object of type `Literal[True]` by zero"

# bitwise OR
reveal_type(a | a)  # revealed: Literal[True]
reveal_type(a | b)  # revealed: Literal[True]
reveal_type(b | a)  # revealed: Literal[True]
reveal_type(b | b)  # revealed: Literal[False]
```

## Arithmetic with a variable

```py
def _(a: bool):
    def lhs_is_int(x: int):
        reveal_type(x + a)  # revealed: int
        reveal_type(x - a)  # revealed: int
        reveal_type(x * a)  # revealed: int
        reveal_type(x // a)  # revealed: int
        reveal_type(x / a)  # revealed: int | float
        reveal_type(x % a)  # revealed: int

    def rhs_is_int(x: int):
        reveal_type(a + x)  # revealed: int
        reveal_type(a - x)  # revealed: int
        reveal_type(a * x)  # revealed: int
        reveal_type(a // x)  # revealed: int
        reveal_type(a / x)  # revealed: int | float
        reveal_type(a % x)  # revealed: int

    def lhs_is_bool(x: bool):
        reveal_type(x + a)  # revealed: int
        reveal_type(x - a)  # revealed: int
        reveal_type(x * a)  # revealed: int
        reveal_type(x // a)  # revealed: int
        reveal_type(x / a)  # revealed: int | float
        reveal_type(x % a)  # revealed: int

    def rhs_is_bool(x: bool):
        reveal_type(a + x)  # revealed: int
        reveal_type(a - x)  # revealed: int
        reveal_type(a * x)  # revealed: int
        reveal_type(a // x)  # revealed: int
        reveal_type(a / x)  # revealed: int | float
        reveal_type(a % x)  # revealed: int

    def both_are_bool(x: bool, y: bool):
        reveal_type(x + y)  # revealed: int
        reveal_type(x - y)  # revealed: int
        reveal_type(x * y)  # revealed: int
        reveal_type(x // y)  # revealed: int
        reveal_type(x / y)  # revealed: int | float
        reveal_type(x % y)  # revealed: int
```
