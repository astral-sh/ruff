# Known constants

## `typing.TYPE_CHECKING`

This constant is `True` when in type-checking mode, `False` otherwise. The symbol is defined to be
`False` at runtime. In typeshed, it is annotated as `bool`. This test makes sure that we infer
`Literal[True]` for it anyways.

### Basic

```py
from typing import TYPE_CHECKING
import typing

reveal_type(TYPE_CHECKING)  # revealed: Literal[True]
reveal_type(typing.TYPE_CHECKING)  # revealed: Literal[True]
```

### Aliased

Make sure that we still infer the correct type if the constant has been given a different name:

```py
from typing import TYPE_CHECKING as TC

reveal_type(TC)  # revealed: Literal[True]
```

### Must originate from `typing`

Make sure we only use our special handling for `typing.TYPE_CHECKING` and not for other constants
with the same name:

```py path=constants.py
TYPE_CHECKING: bool = False
```

```py
from constants import TYPE_CHECKING

reveal_type(TYPE_CHECKING)  # revealed: bool
```

### `typing_extensions` re-export

This should behave in the same way as `typing.TYPE_CHECKING`:

```py
from typing_extensions import TYPE_CHECKING

reveal_type(TYPE_CHECKING)  # revealed: Literal[True]
```
