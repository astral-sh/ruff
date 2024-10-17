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
class MyInt(int):
    def __truediv__(self, other):
        return 100

def returns_int() -> int:
    return MyInt(3)

# TODO: `a` should be `int` and `e` should be `float` once we support inference.
a = 1 / 0  # error: "Cannot divide object of type `Literal[1]` by zero"
b = 2 // 0  # error: "Cannot floor divide object of type `Literal[2]` by zero"
c = 3 % 0  # error: "Cannot reduce object of type `Literal[3]` modulo zero"
# even `int` type could be a subclass of `int` with custom behavior; no error
d = returns_int() / 0
# this could be flagged as an error, if we had an ExactFloat or ExactInstance
# type, but given only a `float` type we can't issue an error for the same
# reason: could be a custom float subclass
e = 1.0 / 0

reveal_type(a)  # revealed: float
reveal_type(b)  # revealed: int
reveal_type(c)  # revealed: int
reveal_type(d)  # revealed: @Todo
reveal_type(e)  # revealed: @Todo
```
