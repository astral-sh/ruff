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
    result = 1 in x  # snapshot
    reveal_type(result)  # revealed: bool
```

```snapshot
error[unsupported-operator]: Unsupported `in` operation
  --> src/mdtest_snippet.py:10:14
   |
10 |     result = 1 in x  # snapshot
   |              -^^^^-
   |              |    |
   |              |    Has type `list[int] | Literal[1]`
   |              Has type `Literal[1]`
   |
info: Operation fails because operator `in` is not supported between two objects of type `Literal[1]`
```

```py
    result2 = y in x  # snapshot: unsupported-operator
    reveal_type(result)  # revealed: bool
```

```snapshot
error[unsupported-operator]: Unsupported `in` operation
  --> src/mdtest_snippet.py:12:15
   |
12 |     result2 = y in x  # snapshot: unsupported-operator
   |               -^^^^-
   |               |
   |               Both operands have type `list[int] | Literal[1]`
   |
info: Operation fails because operator `in` is not supported between objects of type `list[int]` and `Literal[1]`
```

```py
    result3 = aa < cc  # snapshot: unsupported-operator
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:14:15
   |
14 |     result3 = aa < cc  # snapshot: unsupported-operator
   |               --^^^--
   |               |    |
   |               |    Has type `tuple[str] | tuple[str, str]`
   |               Has type `tuple[int]`
   |
info: Operation fails because operator `<` is not supported between objects of type `int` and `str`
```

```py
    result4 = cc < aa  # snapshot: unsupported-operator
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:15:15
   |
15 |     result4 = cc < aa  # snapshot: unsupported-operator
   |               --^^^--
   |               |    |
   |               |    Has type `tuple[int]`
   |               Has type `tuple[str] | tuple[str, str]`
   |
info: Operation fails because operator `<` is not supported between objects of type `str` and `int`
```

```py
    result5 = bb < cc  # snapshot: unsupported-operator
```

```snapshot
error[unsupported-operator]: Unsupported `<` operation
  --> src/mdtest_snippet.py:16:15
   |
16 |     result5 = bb < cc  # snapshot: unsupported-operator
   |               --^^^--
   |               |    |
   |               |    Has type `tuple[str] | tuple[str, str]`
   |               Has type `tuple[int] | tuple[int, int]`
   |
info: Operation fails because operator `<` is not supported between objects of type `int` and `str`
```
