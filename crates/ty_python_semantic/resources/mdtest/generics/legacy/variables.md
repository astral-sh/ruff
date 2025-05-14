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
reveal_type(type(T))  # revealed: <class 'TypeVar'>
reveal_type(T)  # revealed: typing.TypeVar
reveal_type(T.__name__)  # revealed: Literal["T"]
```

### Directly assigned to a variable

> A `TypeVar()` expression must always directly be assigned to a variable (it should not be used as
> part of a larger expression).

```py
from typing import TypeVar

T = TypeVar("T")
# TODO: no error
# error: [invalid-legacy-type-variable]
U: TypeVar = TypeVar("U")

# error: [invalid-legacy-type-variable] "A legacy `typing.TypeVar` must be immediately assigned to a variable"
# error: [invalid-type-form] "Function calls are not allowed in type expressions"
TestList = list[TypeVar("W")]
```

### `TypeVar` parameter must match variable name

> The argument to `TypeVar()` must be a string equal to the variable name to which it is assigned.

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable] "The name of a legacy `typing.TypeVar` (`Q`) must match the name of the variable it is assigned to (`T`)"
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

### Using other typevars as a default

```py
from typing import Generic, TypeVar, Union

T = TypeVar("T")
U = TypeVar("U", default=T)
V = TypeVar("V", default=Union[T, U])

class Valid(Generic[T, U, V]): ...

reveal_type(Valid())  # revealed: Valid[Unknown, Unknown, Unknown]
reveal_type(Valid[int]())  # revealed: Valid[int, int, int]
reveal_type(Valid[int, str]())  # revealed: Valid[int, str, int | str]
reveal_type(Valid[int, str, None]())  # revealed: Valid[int, str, None]

# TODO: error, default value for U isn't available in the generic context
class Invalid(Generic[U]): ...
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

### Cannot be both covariant and contravariant

> To facilitate the declaration of container types where covariant or contravariant type checking is
> acceptable, type variables accept keyword arguments `covariant=True` or `contravariant=True`. At
> most one of these may be passed.

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", covariant=True, contravariant=True)
```

### Variance parameters must be unambiguous

```py
from typing import TypeVar

def cond() -> bool:
    return True

# error: [invalid-legacy-type-variable]
T = TypeVar("T", covariant=cond())

# error: [invalid-legacy-type-variable]
U = TypeVar("U", contravariant=cond())
```

[generics]: https://typing.python.org/en/latest/spec/generics.html
