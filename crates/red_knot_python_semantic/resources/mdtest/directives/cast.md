# `cast`

`cast()` takes two arguments, one type and one value, and returns a value of the given type.

The (inferred) type of the value and the given type do not need to have any correlations.

```py
from typing import Literal, cast

from knot_extensions import TypeOf

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

# TODO: Better error message
cast(val="foo", typ=int)  # error: [unresolved-reference]
```
