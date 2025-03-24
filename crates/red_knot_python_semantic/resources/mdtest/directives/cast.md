# `cast`

`cast()` takes two arguments, one type and one value, and returns a value of the given type.

The (inferred) type of the value and the given type do not need to have any correlation.

```py
from typing import Literal, cast

reveal_type(True)  # revealed: Literal[True]
reveal_type(cast(str, True))  # revealed: str
reveal_type(cast("str", True))  # revealed: str

reveal_type(cast(int | str, 1))  # revealed: int | str

reveal_type(cast(val="foo", typ=int))  # revealed: int

# error: [invalid-type-form]
reveal_type(cast(Literal, True))  # revealed: Unknown

# error: [invalid-type-form]
reveal_type(cast(1, True))  # revealed: Unknown

# error: [missing-argument] "No argument provided for required parameter `val` of function `cast`"
cast(str)
# error: [too-many-positional-arguments] "Too many positional arguments to function `cast`: expected 2, got 3"
cast(str, b"ar", "foo")
```
