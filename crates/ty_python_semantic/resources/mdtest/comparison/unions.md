# Comparison: Unions

## Union on one side of the comparison

Comparisons on union types need to consider all possible cases:

```py
from typing import Literal

def _(
    one_or_two: Literal[1, 2],
    a_or_ab: Literal["a", "ab"],
    one_or_none: Literal[1] | None,
):
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

    reveal_type(a_or_ab in "ab")  # revealed: Literal[True]
    reveal_type("a" in a_or_ab)  # revealed: Literal[True]

    reveal_type("c" not in a_or_ab)  # revealed: Literal[True]
    reveal_type("a" not in a_or_ab)  # revealed: Literal[False]

    reveal_type("b" in a_or_ab)  # revealed: bool
    reveal_type("b" not in a_or_ab)  # revealed: bool

    reveal_type(one_or_none is None)  # revealed: bool
    reveal_type(one_or_none is not None)  # revealed: bool
```

## Union on both sides of the comparison

With unions on both sides, the result type accounts for all possible pairs of values:

```py
from typing import Literal

def _(small: Literal[1, 2], large: Literal[2, 3]):
    reveal_type(small <= large)  # revealed: Literal[True]
    reveal_type(small >= large)  # revealed: bool

    reveal_type(small < large)  # revealed: bool
    reveal_type(small > large)  # revealed: Literal[False]
```

## Equality of string literal unions

Two values drawn from the same union can still differ. Disjoint unions can only compare unequal.

```py
from typing import Literal

def _(
    value: Literal["a", "b"],
    same: Literal["a", "b"],
    overlapping: Literal["b", "c"],
    disjoint: Literal["c", "d"],
):
    reveal_type(value == same)  # revealed: bool
    reveal_type(value != same)  # revealed: bool
    reveal_type(value == overlapping)  # revealed: bool
    reveal_type(value != overlapping)  # revealed: bool
    reveal_type(value == disjoint)  # revealed: Literal[False]
    reveal_type(value != disjoint)  # revealed: Literal[True]
```

## Comparing wide literal unions

For two wide unions of integers, ty can infer `bool` without determining the ordering of each pair.
Comparisons against a single literal still narrow precisely.

```py
from typing import Literal

Digit = Literal[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]

def _(tens: Digit, ones: Digit, other_ones: Digit, missing: bool):
    low = 10 * tens + ones  # 0 through 99
    high = 100 + low  # 100 through 199
    reveal_type(low != high)  # revealed: Literal[True]
    reveal_type(low < high)  # revealed: bool

    if low == 42:
        reveal_type(low)  # revealed: Literal[42]
```

If the overlap is itself wide, narrowing by builtin type can retain values outside it.

```py
    shifted = 10 * tens + other_ones + 1  # 1 through 100
    if low == shifted:
        reveal_type(low == 0)  # revealed: bool
```

A possible `None` makes ordering unsupported. In a tuple comparison, an unsupported pair matters
only when the preceding elements can be equal.

```py
    maybe_low = None if missing else low
    reveal_type(maybe_low == low)  # revealed: bool
    maybe_low < high  # error: [unsupported-operator]
    if maybe_low == low:
        maybe_low.bit_length()  # ok
    if maybe_low is low:
        maybe_low.bit_length()  # ok

    maybe_high = None if missing else high
    if maybe_low == maybe_high:
        reveal_type(maybe_low)  # revealed: None

    true_or_high = True if missing else high  # True compares equal to 1 in low.
    reveal_type(true_or_high == low)  # revealed: bool
    reveal_type(true_or_high is low)  # revealed: Literal[False]

    reveal_type((low,) == (high,))  # revealed: Literal[False]
    (low, None) < (high, 1)  # ok
    (low, None) < (low, 1)  # error: [unsupported-operator]
```

An identity check preserves the order of the shared strings from the right operand. When those
strings were inferred from expressions, they still widen to `str` in a list.

```py
def _(tens: Digit, ones: Digit, flag: bool, kind: Literal["a", "b"]):
    number = 10 * tens + ones
    left = kind if flag else number
    label = "b" if ones else "a"
    right = label if flag else 100 + number
    if left is right:
        reveal_type(left)  # revealed: Literal["b", "a"]
        reveal_type([left])  # revealed: list[str]
```

A `bool` in a wide union can narrow even when all the other union members match. Here the integers
on the right range from 1 through 100, so `True` is the only boolean that compares equal.

```py
def _(tens: Digit, ones: Digit, high_digit: Literal[1, 2, 3, 4, 5], boolean: bool):
    left = 10 * high_digit + ones + 1 if tens else boolean  # bool or 11 through 60
    right = 10 * tens + ones + 1  # 1 through 100
    if left == right and isinstance(left, bool):
        reveal_type(left)  # revealed: Literal[True]

    false_or_high = False if ones else right + 100
    if left is false_or_high:
        reveal_type(left)  # revealed: Literal[False]
```

A custom reflected comparison can contribute its own return type alongside the literal alternatives.

```py
class Reflected:
    def __gt__(self, other: int) -> bytes:
        return b""

def _(tens: Digit, ones: Digit, use_custom: bool, custom: Reflected):
    value = 10 * tens + ones
    other = custom if use_custom else value
    reveal_type(value < other)  # revealed: bytes | bool
```

## Comparing wide string and bytes literal unions

Concatenating two of nine letters gives 81 possible values. Adding a prefix makes a disjoint union.
Equality identifies the disjoint values, while substring tests can infer `bool` even when every
needle is longer than every haystack.

```py
from typing import Literal

Letter = Literal["a", "b", "c", "d", "e", "f", "g", "h", "i"]
ByteLetter = Literal[b"a", b"b", b"c", b"d", b"e", b"f", b"g", b"h", b"i"]

def _(first: Letter, second: Letter, first_byte: ByteLetter, second_byte: ByteLetter, flag: bool):
    text = first + second
    data = first_byte + second_byte
    prefixed_text = "_" + text
    prefixed_data = b"_" + data
    reveal_type(prefixed_text == text)  # revealed: Literal[False]
    reveal_type(prefixed_data != data)  # revealed: Literal[True]
    reveal_type(prefixed_text in text)  # revealed: bool
    reveal_type(prefixed_data not in data)  # revealed: bool
```

When there is one shared two-letter value, equality narrows to it.

```py
    if text == ("ab" if flag else prefixed_text):
        reveal_type(text)  # revealed: Literal["ab"]
    if data == (b"ab" if flag else prefixed_data):
        reveal_type(data)  # revealed: Literal[b"ab"]
```

A text needle is unsupported when the haystack can be `bytes`.

```py
    haystack = text if flag else data
    text in haystack  # error: [unsupported-operator]
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
info: Operation fails because operator `<` is not supported between objects of type `int` and `str`
```
