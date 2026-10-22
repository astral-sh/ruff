# Comparison: Strings

## String literals

```py
def _(x: str):
    reveal_type("abc" == "abc")  # revealed: Literal[True]
    reveal_type("ab_cd" <= "ab_ce")  # revealed: Literal[True]
    reveal_type("abc" in "ab cd")  # revealed: Literal[False]
    reveal_type("" not in "hello")  # revealed: Literal[False]
    reveal_type("--" is "--")  # revealed: bool
    reveal_type("A" is "B")  # revealed: Literal[False]
    reveal_type("--" is not "--")  # revealed: bool
    reveal_type("A" is not "B")  # revealed: Literal[True]
    reveal_type(x < "...")  # revealed: bool

    # ensure we're not comparing the interned salsa symbols, which compare by order of declaration.
    reveal_type("ab" < "ab_cd")  # revealed: Literal[True]
```

## Mismatched literal kinds

Exact built-in literals of different kinds compare unequal. A `LiteralString` can equal any
particular string literal, so those comparisons remain ambiguous.

```py
from typing_extensions import LiteralString

reveal_type(True == "")  # revealed: Literal[False]
reveal_type(True != "")  # revealed: Literal[True]
reveal_type(b"" == "")  # revealed: Literal[False]
reveal_type(b"" != "")  # revealed: Literal[True]

def _(value: LiteralString):
    reveal_type(value == 1)  # revealed: Literal[False]
    reveal_type(b"" != value)  # revealed: Literal[True]
    reveal_type(value == "")  # revealed: bool
    reveal_type(value != "")  # revealed: bool
```

## Equality with sequences

A `Sequence[str]` can be a string, including the empty string, so comparing it with a string literal
has an unknown result:

```py
from collections.abc import Sequence

def _(value: Sequence[str]):
    reveal_type(value == "")  # revealed: bool
    reveal_type("" == value)  # revealed: bool
    reveal_type(value != "")  # revealed: bool
    reveal_type("" != value)  # revealed: bool
```

The result is also unknown when the string's precise literal value is not known:

```py
from typing_extensions import LiteralString

def _(value: Sequence[str], other: LiteralString):
    reveal_type(value == other)  # revealed: bool
    reveal_type(other == value)  # revealed: bool
    reveal_type(value != other)  # revealed: bool
    reveal_type(other != value)  # revealed: bool
```
