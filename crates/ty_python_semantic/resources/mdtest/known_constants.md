# Known constants

## `typing.TYPE_CHECKING`

This constant is `True` when in type-checking mode, `False` otherwise. The symbol is defined to be
`False` at runtime. In typeshed, it is annotated as `bool`.

### Basic

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    type_checking = True
if not TYPE_CHECKING:
    runtime = True

# type_checking is treated as unconditionally assigned.
reveal_type(type_checking)  # revealed: Literal[True]
# error: [unresolved-reference]
reveal_type(runtime)  # revealed: Unknown
```

### As module attribute

```py
import typing

if typing.TYPE_CHECKING:
    type_checking = True
if not typing.TYPE_CHECKING:
    runtime = True

reveal_type(type_checking)  # revealed: Literal[True]
# error: [unresolved-reference]
reveal_type(runtime)  # revealed: Unknown
```

### `typing_extensions` re-export

This should behave in the same way as `typing.TYPE_CHECKING`:

```py
from typing_extensions import TYPE_CHECKING

if TYPE_CHECKING:
    type_checking = True
if not TYPE_CHECKING:
    runtime = True

reveal_type(type_checking)  # revealed: Literal[True]
# error: [unresolved-reference]
reveal_type(runtime)  # revealed: Unknown
```

## User-defined `TYPE_CHECKING`

If we set `TYPE_CHECKING = False` directly instead of importing it from the `typing` module, it will
still be treated as `True` during type checking. This behavior is for compatibility with other major
type checkers, e.g. mypy and pyright.

### With no type annotation

```py
TYPE_CHECKING = False

if TYPE_CHECKING:
    type_checking = True
if not TYPE_CHECKING:
    runtime = True

# type_checking is treated as unconditionally assigned.
reveal_type(type_checking)  # revealed: Literal[True]
# error: [unresolved-reference]
reveal_type(runtime)  # revealed: Unknown
```

### With a type annotation

We can also define `TYPE_CHECKING` with a type annotation. The type must be one to which `bool` can
be assigned.

```py
TYPE_CHECKING: bool = False

if TYPE_CHECKING:
    type_checking = True
if not TYPE_CHECKING:
    runtime = True

reveal_type(type_checking)  # revealed: Literal[True]
# error: [unresolved-reference]
reveal_type(runtime)  # revealed: Unknown
```

### Importing user-defined `TYPE_CHECKING`

`constants.py`:

```py
TYPE_CHECKING = False
```

```py
from constants import TYPE_CHECKING

if TYPE_CHECKING:
    type_checking = True
if not TYPE_CHECKING:
    runtime = True

reveal_type(type_checking)  # revealed: Literal[True]
# error: [unresolved-reference]
reveal_type(runtime)  # revealed: Unknown
```

### Importing user-defined `TYPE_CHECKING` from stub

`stub.pyi`:

```pyi
TYPE_CHECKING: bool
# or
TYPE_CHECKING: bool = ...
```

```py
from stub import TYPE_CHECKING

if TYPE_CHECKING:
    type_checking = True
if not TYPE_CHECKING:
    runtime = True

reveal_type(type_checking)  # revealed: Literal[True]
# error: [unresolved-reference]
reveal_type(runtime)  # revealed: Unknown
```

### Invalid assignment to `TYPE_CHECKING`

Only `False` can be assigned to `TYPE_CHECKING`; any assignment other than `False` will result in an
error. A type annotation to which `bool` is not assignable is also an error.

```py
from typing import Literal

# error: [invalid-type-checking-constant]
TYPE_CHECKING = True

# error: [invalid-type-checking-constant]
TYPE_CHECKING: bool = True

# error: [invalid-type-checking-constant]
TYPE_CHECKING: int = 1

# error: [invalid-type-checking-constant]
TYPE_CHECKING: str = "str"

# error: [invalid-assignment]
# error: [invalid-type-checking-constant]
TYPE_CHECKING: str = False

# error: [invalid-type-checking-constant]
TYPE_CHECKING: Literal[False] = False

# error: [invalid-assignment]
# error: [invalid-type-checking-constant]
TYPE_CHECKING: Literal[True] = False
```

The same rules apply in a stub file:

```pyi
from typing import Literal

# error: [invalid-type-checking-constant]
TYPE_CHECKING: str

# error: [invalid-assignment]
# error: [invalid-type-checking-constant]
TYPE_CHECKING: str = False

# error: [invalid-type-checking-constant]
TYPE_CHECKING: Literal[False] = ...

# error: [invalid-type-checking-constant]
TYPE_CHECKING: object = "str"
```
