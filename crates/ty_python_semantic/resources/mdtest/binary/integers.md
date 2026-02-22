# Binary operations on integers

## Basic Arithmetic

```py
reveal_type(2 + 1)  # revealed: Literal[3]
reveal_type(3 - 4)  # revealed: Literal[-1]
reveal_type(3 * -1)  # revealed: Literal[-3]
reveal_type(-3 // 3)  # revealed: Literal[-1]
reveal_type(-3 / 3)  # revealed: float
reveal_type(5 % 3)  # revealed: Literal[2]
reveal_type(3 | 4)  # revealed: Literal[7]
reveal_type(5 & 6)  # revealed: Literal[4]
reveal_type(7 ^ 2)  # revealed: Literal[5]

# error: [unsupported-operator] "Operator `+` is not supported between objects of type `Literal[2]` and `Literal["f"]`"
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

# Edge case: the runtime value is 9223372036854775808,
# which doesn't fit into an i64
reveal_type(-(-9223372036854775807 - 1))  # revealed: int
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
    reveal_type(x**2)  # revealed: int
    reveal_type(2**x)  # revealed: Any
    reveal_type(x**x)  # revealed: Any
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

## Division and Modulus

Division works differently in Python than in Rust. If the result is negative and there is a
remainder, the division rounds down (instead of towards zero). The remainder needs to be adjusted to
compensate so that `(lhs // rhs) * rhs + (lhs % rhs) == lhs`:

```py
reveal_type(256 % 129)  # revealed: Literal[127]
reveal_type(-256 % 129)  # revealed: Literal[2]
reveal_type(256 % -129)  # revealed: Literal[-2]
reveal_type(-256 % -129)  # revealed: Literal[-127]

reveal_type(129 % 16)  # revealed: Literal[1]
reveal_type(-129 % 16)  # revealed: Literal[15]
reveal_type(129 % -16)  # revealed: Literal[-15]
reveal_type(-129 % -16)  # revealed: Literal[-1]

reveal_type(10 // 8)  # revealed: Literal[1]
reveal_type(-10 // 8)  # revealed: Literal[-2]
reveal_type(10 // -8)  # revealed: Literal[-2]
reveal_type(-10 // -8)  # revealed: Literal[1]

reveal_type(10 // 6)  # revealed: Literal[1]
reveal_type(-10 // 6)  # revealed: Literal[-2]
reveal_type(10 // -6)  # revealed: Literal[-2]
reveal_type(-10 // -6)  # revealed: Literal[1]
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

## Bit-shifting

Literal arithmetic is supported for bit-shifting operations on `int`s:

```py
reveal_type(42 << 3)  # revealed: Literal[336]
reveal_type(0 << 3)  # revealed: Literal[0]
reveal_type(-42 << 3)  # revealed: Literal[-336]

reveal_type(42 >> 3)  # revealed: Literal[5]
reveal_type(0 >> 3)  # revealed: Literal[0]
reveal_type(-42 >> 3)  # revealed: Literal[-6]
reveal_type(1 >> 5000000000)  # revealed: Literal[0]
reveal_type(-1 >> 5000000000)  # revealed: Literal[-1]
```

If the result of a left shift overflows the `int` literal type, it becomes `int`. This includes
cases where a bit is shifted into the sign bit position, since Python integers have arbitrary
precision and `1 << 63` is a large positive number, not a negative one. Right shifts do not
overflow:

```py
# For `0` specifically, we know that any right-shift
# will produce `0`
reveal_type(0 << 4000000000000000000)  # revealed: Literal[0]

reveal_type(1 << 62)  # revealed: Literal[4611686018427387904]
reveal_type(1 << 63)  # revealed: int

reveal_type(2 << 61)  # revealed: Literal[4611686018427387904]
reveal_type(2 << 63)  # revealed: int

reveal_type(-1 << 63)  # revealed: Literal[-9223372036854775808]
reveal_type(-1 << 64)  # revealed: int

# Larger values: the headroom depends on the number of significant bits in `n`,
# not on the value of `n` itself.
reveal_type(100 << 3)  # revealed: Literal[800]
reveal_type(100 << 56)  # revealed: Literal[7205759403792793600]
reveal_type(100 << 57)  # revealed: int

# Negative values with large shifts that would overflow i64:
reveal_type(-3 << 61)  # revealed: Literal[-6917529027641081856]
reveal_type(-3 << 62)  # revealed: int
reveal_type(-100 << 56)  # revealed: Literal[-7205759403792793600]
reveal_type(-100 << 57)  # revealed: int
```

Shifting by a negative amount is a `ValueError` at runtime. We don't emit a diagnostic for this
currently; we simply infer `int` as the result type:

```py
reveal_type(42 << -3)  # revealed: int
reveal_type(0 << -3)  # revealed: int
reveal_type(-42 << -3)  # revealed: int

reveal_type(42 >> -3)  # revealed: int
reveal_type(0 >> -3)  # revealed: int
reveal_type(-42 >> -3)  # revealed: int
```
