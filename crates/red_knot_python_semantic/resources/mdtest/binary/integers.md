# Binary operations on integers

## Basic Arithmetic

```py
reveal_type(2 + 1)  # revealed: Literal[3]
reveal_type(3 - 4)  # revealed: Literal[-1]
reveal_type(3 * -1)  # revealed: Literal[-3]
reveal_type(-3 // 3)  # revealed: Literal[-1]
reveal_type(-3 / 3)  # revealed: float
reveal_type(5 % 3)  # revealed: Literal[2]

# error: [unsupported-operator] "Operator `+` is unsupported between objects of type `Literal[2]` and `Literal["f"]`"
reveal_type(2 + "f")  # revealed: Unknown

def lhs(x: int):
    reveal_type(x + 1)  # revealed: int
    reveal_type(x - 4)  # revealed: int
    reveal_type(x * -1)  # revealed: int
    reveal_type(x // 3)  # revealed: int
    reveal_type(x / 3)  # revealed: int | float
    reveal_type(x % 3)  # revealed: int

def rhs(x: int):
    reveal_type(2 + x)  # revealed: int
    reveal_type(3 - x)  # revealed: int
    reveal_type(3 * x)  # revealed: int
    reveal_type(-3 // x)  # revealed: int
    reveal_type(-3 / x)  # revealed: int | float
    reveal_type(5 % x)  # revealed: int

def both(x: int):
    reveal_type(x + x)  # revealed: int
    reveal_type(x - x)  # revealed: int
    reveal_type(x * x)  # revealed: int
    reveal_type(x // x)  # revealed: int
    reveal_type(x / x)  # revealed: int | float
    reveal_type(x % x)  # revealed: int
```

## Power

For power if the result fits in the int literal type it will be a Literal type. Otherwise the
outcome is int.

```py
largest_u32 = 4_294_967_295
reveal_type(2**2)  # revealed: Literal[4]
reveal_type(1 ** (largest_u32 + 1))  # revealed: int
reveal_type(2**largest_u32)  # revealed: int

def variable(x: int):
    reveal_type(x**2)  # revealed: @Todo(return type of overloaded function)
    reveal_type(2**x)  # revealed: @Todo(return type of overloaded function)
    reveal_type(x**x)  # revealed: @Todo(return type of overloaded function)
```

If the second argument is \<0, a `float` is returned at runtime. If the first argument is \<0 but
the second argument is >=0, an `int` is still returned:

```py
reveal_type(1**0)  # revealed: Literal[1]
reveal_type(0**1)  # revealed: Literal[0]
reveal_type(0**0)  # revealed: Literal[1]
reveal_type((-1) ** 2)  # revealed: Literal[1]
reveal_type(2 ** (-1))  # revealed: float
reveal_type((-1) ** (-1))  # revealed: float
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
reveal_type(a)  # revealed: float

b = 2 // 0  # error: "Cannot floor divide object of type `Literal[2]` by zero"
reveal_type(b)  # revealed: int

c = 3 % 0  # error: "Cannot reduce object of type `Literal[3]` modulo zero"
reveal_type(c)  # revealed: int

# error: "Cannot divide object of type `int` by zero"
reveal_type(int() / 0)  # revealed: int | float

# error: "Cannot divide object of type `Literal[1]` by zero"
reveal_type(1 / False)  # revealed: float
# error: [division-by-zero] "Cannot divide object of type `Literal[True]` by zero"
True / False
# error: [division-by-zero] "Cannot divide object of type `Literal[True]` by zero"
bool(1) / False

# error: "Cannot divide object of type `float` by zero"
reveal_type(1.0 / 0)  # revealed: int | float

class MyInt(int): ...

# No error for a subclass of int
reveal_type(MyInt(3) / 0)  # revealed: int | float
```
