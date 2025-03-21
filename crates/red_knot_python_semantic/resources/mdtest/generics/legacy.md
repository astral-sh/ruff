# Legacy type variables

The tests in this file focus on how type variables are defined using the legacy notation. Most
_uses_ of type variables are tested in other files in this directory; we do not duplicate every test
for both type variable syntaxes.

Unless otherwise specified, all quotations come from the [Generics] section of the typing spec.

## Type variables

### Defining legacy type variables

> Generics can be parameterized by using a factory available in `typing` called `TypeVar`.

This was the only way to create type variables prior to PEP 695/Python 3.12. It is still available
in newer Python releases.

```py
from typing import TypeVar

T = TypeVar("T")
reveal_type(type(T))  # revealed: Literal[TypeVar]
reveal_type(T)  # revealed: T
reveal_type(T.__name__)  # revealed: Literal["T"]
```

### Directly assigned to a variable

> A `TypeVar()` expression must always directly be assigned to a variable (it should not be used as
> part of a larger expression).

```py
from typing import TypeVar

# TODO: error
TestList = list[TypeVar("W")]
```

### `TypeVar` parameter must match variable name

> The argument to `TypeVar()` must be a string equal to the variable name to which it is assigned.

```py
from typing import TypeVar

# TODO: error
T = TypeVar("Q")
```

### No redefinition

> Type variables must not be redefined.

```py
from typing import TypeVar

T = TypeVar("T")

# TODO: error
T = TypeVar("T")
```

### Type variables with a default

Note that the `__default__` property is only available in Python ≥3.13.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import TypeVar

T = TypeVar("T", default=int)
reveal_type(T.__default__)  # revealed: int
reveal_type(T.__bound__)  # revealed: None
reveal_type(T.__constraints__)  # revealed: tuple[()]

S = TypeVar("S")
reveal_type(S.__default__)  # revealed: NoDefault
```

### Type variables with an upper bound

```py
from typing import TypeVar

T = TypeVar("T", bound=int)
reveal_type(T.__bound__)  # revealed: int
reveal_type(T.__constraints__)  # revealed: tuple[()]

S = TypeVar("S")
reveal_type(S.__bound__)  # revealed: None
```

### Type variables with constraints

```py
from typing import TypeVar

T = TypeVar("T", int, str)
reveal_type(T.__constraints__)  # revealed: tuple[int, str]

S = TypeVar("S")
reveal_type(S.__constraints__)  # revealed: tuple[()]
```

### Cannot have only one constraint

> `TypeVar` supports constraining parametric types to a fixed set of possible types...There should
> be at least two constraints, if any; specifying a single constraint is disallowed.

```py
from typing import TypeVar

# TODO: error: [invalid-type-variable-constraints]
T = TypeVar("T", int)
```

[generics]: https://typing.readthedocs.io/en/latest/spec/generics.html
