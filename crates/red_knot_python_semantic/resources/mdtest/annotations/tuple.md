# Tuple

## `Never`

If a tuple type contains a `Never` element, then it is eagerly simplified to `Never` which means
that a tuple type containing `Never` is disjoint from any other tuple type.

```py
from typing_extensions import Never


def _(x: tuple[Never], y: tuple[int, Never], z: tuple[Never, int]):
    reveal_type(x)  # revealed: Never
    reveal_type(y)  # revealed: Never
    reveal_type(z)  # revealed: Never
```
