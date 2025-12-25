# Legacy typevar creation diagnostics

The full tests for these features are in `generics/legacy/variables.md`.

<!-- snapshot-diagnostics -->

## Must have a name

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar()
```

## Name can't be given more than once

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", name="T")
```

## Must be directly assigned to a variable

> A `TypeVar()` expression must always directly be assigned to a variable (it should not be used as
> part of a larger expression).

```py
from typing import TypeVar

T = TypeVar("T")
# error: [invalid-legacy-type-variable]
U: TypeVar = TypeVar("U")

# error: [invalid-legacy-type-variable]
tuple_with_typevar = ("foo", TypeVar("W"))
```

## `TypeVar` parameter must match variable name

> The argument to `TypeVar()` must be a string equal to the variable name to which it is assigned.

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("Q")
```

## No variadic arguments

```py
from typing import TypeVar

types = (int, str)

# error: [invalid-legacy-type-variable]
T = TypeVar("T", *types)

# error: [invalid-legacy-type-variable]
S = TypeVar("S", **{"bound": int})
```

## Cannot have only one constraint

> `TypeVar` supports constraining parametric types to a fixed set of possible types...There should
> be at least two constraints, if any; specifying a single constraint is disallowed.

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", int)
```

## Cannot have both bound and constraint

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", int, str, bound=bytes)
```

## Cannot be both covariant and contravariant

> To facilitate the declaration of container types where covariant or contravariant type checking is
> acceptable, type variables accept keyword arguments `covariant=True` or `contravariant=True`. At
> most one of these may be passed.

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", covariant=True, contravariant=True)
```

## Boolean parameters must be unambiguous

```py
from typing_extensions import TypeVar

def cond() -> bool:
    return True

# error: [invalid-legacy-type-variable]
T = TypeVar("T", covariant=cond())

# error: [invalid-legacy-type-variable]
U = TypeVar("U", contravariant=cond())

# error: [invalid-legacy-type-variable]
V = TypeVar("V", infer_variance=cond())
```

## Invalid keyword arguments

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", invalid_keyword=True)
```

## Invalid feature for this Python version

```toml
[environment]
python-version = "3.10"
```

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", default=int)
```
