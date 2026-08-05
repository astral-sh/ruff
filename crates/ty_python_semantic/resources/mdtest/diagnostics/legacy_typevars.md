# Legacy typevar creation diagnostics

The full tests for these features are in `generics/legacy/variables.md`.

## Must have a name

```py
from typing import TypeVar

# snapshot: invalid-legacy-type-variable
T = TypeVar()
```

```snapshot
error[invalid-legacy-type-variable]: The `name` parameter of `TypeVar` is required.
 --> src/mdtest_snippet.py:4:5
  |
4 | T = TypeVar()
  |     ^^^^^^^^^
```

## Name can't be given more than once

```py
from typing import TypeVar

# snapshot: invalid-legacy-type-variable
T = TypeVar("T", name="T")
```

```snapshot
error[invalid-legacy-type-variable]: The `name` parameter of `TypeVar` can only be provided once.
 --> src/mdtest_snippet.py:4:18
  |
4 | T = TypeVar("T", name="T")
  |                  ^^^^^^^^
```

## Must be directly assigned to a variable

> A `TypeVar()` expression must always directly be assigned to a variable (it should not be used as
> part of a larger expression).

```py
from typing import TypeVar

T = TypeVar("T")
# snapshot: invalid-legacy-type-variable
U: TypeVar = TypeVar("U")

# snapshot: invalid-legacy-type-variable
tuple_with_typevar = ("foo", TypeVar("W"))
```

```snapshot
error[invalid-legacy-type-variable]: A `TypeVar` definition must be a simple variable assignment
 --> src/mdtest_snippet.py:5:14
  |
5 | U: TypeVar = TypeVar("U")
  |              ^^^^^^^^^^^^


error[invalid-legacy-type-variable]: A `TypeVar` definition must be a simple variable assignment
 --> src/mdtest_snippet.py:8:30
  |
8 | tuple_with_typevar = ("foo", TypeVar("W"))
  |                              ^^^^^^^^^^^^
```

## `TypeVar` parameter must match variable name

> The argument to `TypeVar()` must be a string equal to the variable name to which it is assigned.

```py
from typing import TypeVar

# snapshot: mismatched-type-name
T = TypeVar("Q")
```

```snapshot
warning[mismatched-type-name]: The name passed to `TypeVar` must match the variable it is assigned to
 --> src/mdtest_snippet.py:4:13
  |
4 | T = TypeVar("Q")
  |             ^^^ Expected "T", got "Q"
```

## Must not be redefined

```py
from typing import TypeVar

T = TypeVar("T")

# snapshot: invalid-legacy-type-variable
T = TypeVar("T")
```

```snapshot
error[invalid-legacy-type-variable]: Cannot redefine `T` as a type variable
 --> src/mdtest_snippet.py:6:1
  |
3 | T = TypeVar("T")
  | - Previously defined here
4 |
5 | # snapshot: invalid-legacy-type-variable
6 | T = TypeVar("T")
  | ^
```

## No variadic arguments

```py
from typing import TypeVar

types = (int, str)

# snapshot: invalid-legacy-type-variable
T = TypeVar("T", *types)

# snapshot: invalid-legacy-type-variable
S = TypeVar("S", **{"bound": int})
```

```snapshot
error[invalid-legacy-type-variable]: Starred arguments are not supported in `TypeVar` creation
 --> src/mdtest_snippet.py:6:18
  |
6 | T = TypeVar("T", *types)
  |                  ^^^^^^


error[invalid-legacy-type-variable]: Starred arguments are not supported in `TypeVar` creation
 --> src/mdtest_snippet.py:9:18
  |
9 | S = TypeVar("S", **{"bound": int})
  |                  ^^^^^^^^^^^^^^^^
```

## Invalid keyword arguments

```py
from typing import TypeVar

# snapshot: invalid-legacy-type-variable
T = TypeVar("T", invalid_keyword=True)
```

```snapshot
error[invalid-legacy-type-variable]: Unknown keyword argument `invalid_keyword` in `TypeVar` creation
 --> src/mdtest_snippet.py:4:18
  |
4 | T = TypeVar("T", invalid_keyword=True)
  |                  ^^^^^^^^^^^^^^^^^^^^
```

## Invalid feature for this Python version

```toml
[environment]
python-version = "3.10"
```

```py
from typing import TypeVar

# snapshot: invalid-legacy-type-variable
T = TypeVar("T", default=int)
```

```snapshot
error[invalid-legacy-type-variable]: The `default` parameter of `typing.TypeVar` was added in Python 3.13
 --> src/mdtest_snippet.py:4:18
  |
4 | T = TypeVar("T", default=int)
  |                  ^^^^^^^^^^^
```
