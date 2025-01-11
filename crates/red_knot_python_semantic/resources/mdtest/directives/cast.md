# `cast`

`cast()` takes two arguments, one type and one value, and returns a value of the given type.

The (inferred) type of the value and the given type do not need to have any correlation.

```py
from typing import Literal, cast

reveal_type(True)  # revealed: Literal[True]
reveal_type(cast(str, True))  # revealed: str
reveal_type(cast("str", True))  # revealed: str

reveal_type(cast(int | str, 1))  # revealed: int | str

# error: [invalid-type-form]
reveal_type(cast(Literal, True))  # revealed: Unknown

# TODO: These should be errors
cast(1)
cast(str)
cast(str, b"ar", "foo")

# TODO: Either support keyword arguments properly,
# or give a comprehensible error message saying they're unsupported
cast(val="foo", typ=int)  # error: [unresolved-reference] "Name `foo` used when not defined"
```
