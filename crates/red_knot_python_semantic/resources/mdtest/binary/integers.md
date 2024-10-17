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

This error is really outside the current Python type system, because e.g. `int.__truediv__` and
friends are not annotated to indicate that it's an error, and we don't even have a facility to
permit such an annotation. So arguably divide-by-zero should be a lint error rather than a type
checker error. But we choose to go ahead and error in the cases that are very likely to be an error:
dividing something typed as `int` or `float` by something known to be `Literal[0]`.

This isn't _definitely_ an error, because the object typed as `int` or `float` could be an instance
of a custom subclass which overrides division behavior to handle zero without error. But if this
unusual case occurs, the error can be avoided by explicitly typing the dividend as that safe custom
subclass; we only emit the error if the LHS type is exactly `int` or `float`, not if its a subclass.

```py
a = 1 / 0  # error: "Cannot divide object of type `Literal[1]` by zero"
b = 2 // 0  # error: "Cannot floor divide object of type `Literal[2]` by zero"
c = 3 % 0  # error: "Cannot reduce object of type `Literal[3]` modulo zero"
d = int() / 0  # error: "Cannot divide object of type `int` by zero"
e = 1.0 / 0  # error: "Cannot divide object of type `float` by zero"

reveal_type(a)  # revealed: float
reveal_type(b)  # revealed: int
reveal_type(c)  # revealed: int
# TODO should be int
reveal_type(d)  # revealed: @Todo
# TODO should be float
reveal_type(e)  # revealed: @Todo

class MyInt(int): pass

# No error for a subclass of int
# TODO should be float
reveal_type(MyInt(3) / 0)  # revealed: @Todo
```
