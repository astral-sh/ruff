# Binary operations on union types

Binary operations on union types are only available if they are supported for all possible
combinations of types:

```py
def f1(i: int, u: int | None):
    # error: [unsupported-operator] "Operator `+` is unsupported between objects of type `int` and `int | None`"
    reveal_type(i + u)  # revealed: Unknown
    # error: [unsupported-operator] "Operator `+` is unsupported between objects of type `int | None` and `int`"
    reveal_type(u + i)  # revealed: Unknown
```

`int` can be added to `int`, and `str` can be added to `str`, but expressions of type `int | str`
cannot be added, because that would require addition of `int` and `str` or vice versa:

```py
def f2(i: int, s: str, int_or_str: int | str):
    i + i
    s + s
    # error: [unsupported-operator] "Operator `+` is unsupported between objects of type `int | str` and `int | str`"
    reveal_type(int_or_str + int_or_str)  # revealed: Unknown
```

However, if an operation is supported for all possible combinations, the result will be a union of
the possible outcomes:

```py
from typing import Literal

def f3(two_or_three: Literal[2, 3], a_or_b: Literal["a", "b"]):
    reveal_type(two_or_three + two_or_three)  # revealed: Literal[4, 5, 6]
    reveal_type(two_or_three**two_or_three)  # revealed: Literal[4, 8, 9, 27]

    reveal_type(a_or_b + a_or_b)  # revealed: Literal["aa", "ab", "ba", "bb"]

    reveal_type(two_or_three * a_or_b)  # revealed: Literal["aa", "bb", "aaa", "bbb"]
```

We treat a type annotation of `float` as a union of `int` and `float`, so union handling is relevant
here:

```py
def f4(x: float, y: float):
    reveal_type(x + y)  # revealed: int | float
    reveal_type(x - y)  # revealed: int | float
    reveal_type(x * y)  # revealed: int | float
    reveal_type(x / y)  # revealed: int | float
    reveal_type(x // y)  # revealed: int | float
    reveal_type(x % y)  # revealed: int | float
```

If any of the union elements leads to a division by zero, we will report an error:

```py
def f5(m: int, n: Literal[-1, 0, 1]):
    # error: [division-by-zero] "Cannot divide object of type `int` by zero"
    return m / n
```
