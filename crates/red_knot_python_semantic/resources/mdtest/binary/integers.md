## Binary operations on integers

## Basic Arithmetic

```py
a = 2 + 1
b = a - 4
c = a * b
d = c // 3
e = c / 3
f = 5 % 3

reveal_type(a)  # revealed: Literal[3]
reveal_type(b)  # revealed: Literal[-1]
reveal_type(c)  # revealed: Literal[-3]
reveal_type(d)  # revealed: Literal[-1]
reveal_type(e)  # revealed: float
reveal_type(f)  # revealed: Literal[2]
```

## Division by Zero

```py
# TODO: `a` should be `int` and `e` should be `float` once we support inference.
a = 1 / 0  # error: "Cannot divide object of type `Literal[1]` by zero"
b = 2 // 0  # error: "Cannot floor divide object of type `Literal[2]` by zero"
c = 3 % 0  # error: "Cannot reduce object of type `Literal[3]` modulo zero"
d = int() / 0  # error: "Cannot divide object of type `int` by zero"
e = 1.0 / 0  # error: "Cannot divide object of type `float` by zero"

reveal_type(a)  # revealed: float
reveal_type(b)  # revealed: int
reveal_type(c)  # revealed: int
reveal_type(d)  # revealed: @Todo
reveal_type(e)  # revealed: @Todo
```
