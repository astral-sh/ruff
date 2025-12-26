# Comparison: Unions

## Union on one side of the comparison

Comparisons on union types need to consider all possible cases:

```py
def _(flag: bool):
    one_or_two = 1 if flag else 2

    reveal_type(one_or_two <= 2)  # revealed: Literal[True]
    reveal_type(one_or_two <= 1)  # revealed: bool
    reveal_type(one_or_two <= 0)  # revealed: Literal[False]

    reveal_type(2 >= one_or_two)  # revealed: Literal[True]
    reveal_type(1 >= one_or_two)  # revealed: bool
    reveal_type(0 >= one_or_two)  # revealed: Literal[False]

    reveal_type(one_or_two < 1)  # revealed: Literal[False]
    reveal_type(one_or_two < 2)  # revealed: bool
    reveal_type(one_or_two < 3)  # revealed: Literal[True]

    reveal_type(one_or_two > 0)  # revealed: Literal[True]
    reveal_type(one_or_two > 1)  # revealed: bool
    reveal_type(one_or_two > 2)  # revealed: Literal[False]

    reveal_type(one_or_two == 3)  # revealed: Literal[False]
    reveal_type(one_or_two == 1)  # revealed: bool

    reveal_type(one_or_two != 3)  # revealed: Literal[True]
    reveal_type(one_or_two != 1)  # revealed: bool

    a_or_ab = "a" if flag else "ab"

    reveal_type(a_or_ab in "ab")  # revealed: Literal[True]
    reveal_type("a" in a_or_ab)  # revealed: Literal[True]

    reveal_type("c" not in a_or_ab)  # revealed: Literal[True]
    reveal_type("a" not in a_or_ab)  # revealed: Literal[False]

    reveal_type("b" in a_or_ab)  # revealed: bool
    reveal_type("b" not in a_or_ab)  # revealed: bool

    one_or_none = 1 if flag else None

    reveal_type(one_or_none is None)  # revealed: bool
    reveal_type(one_or_none is not None)  # revealed: bool
```

## Union on both sides of the comparison

With unions on both sides, we need to consider the full cross product of options when building the
resulting (union) type:

```py
def _(flag_s: bool, flag_l: bool):
    small = 1 if flag_s else 2
    large = 2 if flag_l else 3

    reveal_type(small <= large)  # revealed: Literal[True]
    reveal_type(small >= large)  # revealed: bool

    reveal_type(small < large)  # revealed: bool
    reveal_type(small > large)  # revealed: Literal[False]
```

## Unsupported operations

<!-- snapshot-diagnostics -->

Make sure we emit a diagnostic if *any* of the possible comparisons is unsupported. For now, we fall
back to `bool` for the result type instead of trying to infer something more precise from the other
(supported) variants:

```py
from typing import Literal

def _(
    x: list[int] | Literal[1],
    y: list[int] | Literal[1],
    aa: tuple[int],
    bb: tuple[int] | tuple[int, int],
    cc: tuple[str] | tuple[str, str],
):
    result = 1 in x  # error: "Operator `in` is not supported"
    reveal_type(result)  # revealed: bool

    result2 = y in x  # error: [unsupported-operator]
    reveal_type(result)  # revealed: bool

    result3 = aa < cc  # error: [unsupported-operator]
    result4 = cc < aa  # error: [unsupported-operator]
    result5 = bb < cc  # error: [unsupported-operator]
```
