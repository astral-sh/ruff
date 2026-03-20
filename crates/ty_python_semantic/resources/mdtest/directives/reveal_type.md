# `reveal_type`

`reveal_type` is used to inspect the type of an expression at a given point in the code. It is often
used for debugging and understanding how types are inferred by the type checker.

## Basic usage

```py
from typing_extensions import reveal_type

reveal_type(1)  # revealed: Literal[1]
```

The return type of `reveal_type` is the type of the argument:

```py
from typing_extensions import assert_type

def _(x: int):
    y = reveal_type(x)  # revealed: int
    assert_type(y, int)
```

## Without importing it

For convenience, we also allow `reveal_type` to be used without importing it, even if that would
fail at runtime:

```py
reveal_type(1)  # revealed: Literal[1]
```

## In unreachable code

Make sure that `reveal_type` works even in unreachable code.

### When importing it

```py
from typing_extensions import reveal_type

if False:
    reveal_type(1)  # revealed: Literal[1]

if 1 + 1 != 2:
    reveal_type(1)  # revealed: Literal[1]
```

### Without importing it

```py
if False:
    reveal_type(1)  # revealed: Literal[1]

if 1 + 1 != 2:
    reveal_type(1)  # revealed: Literal[1]
```
