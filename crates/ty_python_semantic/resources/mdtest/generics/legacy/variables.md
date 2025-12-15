# Legacy type variables

The tests in this file focus on how type variables are defined using the legacy notation. Most
_uses_ of type variables are tested in other files in this directory; we do not duplicate every test
for both type variable syntaxes.

Unless otherwise specified, all quotations come from the [Generics] section of the typing spec.

Diagnostics for invalid type variables are snapshotted in `diagnostics/legacy_typevars.md`.

## Type variables

### Defining legacy type variables

> Generics can be parameterized by using a factory available in `typing` called `TypeVar`.

This was the only way to create type variables prior to PEP 695/Python 3.12. It is still available
in newer Python releases.

```py
from typing import TypeVar

T = TypeVar("T")
reveal_type(type(T))  # revealed: <class 'TypeVar'>
reveal_type(T)  # revealed: TypeVar
reveal_type(T.__name__)  # revealed: Literal["T"]
```

The typevar name can also be provided as a keyword argument:

```py
from typing import TypeVar

T = TypeVar(name="T")
reveal_type(T.__name__)  # revealed: Literal["T"]
```

### Must be directly assigned to a variable

> A `TypeVar()` expression must always directly be assigned to a variable (it should not be used as
> part of a larger expression).

```py
from typing import TypeVar

T = TypeVar("T")
# error: [invalid-legacy-type-variable]
U: TypeVar = TypeVar("U")

# error: [invalid-legacy-type-variable]
tuple_with_typevar = ("foo", TypeVar("W"))
reveal_type(tuple_with_typevar[1])  # revealed: TypeVar
```

```py
from typing_extensions import TypeVar

T = TypeVar("T")
# error: [invalid-legacy-type-variable]
U: TypeVar = TypeVar("U")

# error: [invalid-legacy-type-variable]
tuple_with_typevar = ("foo", TypeVar("W"))
reveal_type(tuple_with_typevar[1])  # revealed: TypeVar
```

### `TypeVar` parameter must match variable name

> The argument to `TypeVar()` must be a string equal to the variable name to which it is assigned.

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
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

### No variadic arguments

```py
from typing import TypeVar

types = (int, str)

# error: [invalid-legacy-type-variable]
T = TypeVar("T", *types)
reveal_type(T)  # revealed: TypeVar

# error: [invalid-legacy-type-variable]
S = TypeVar("S", **{"bound": int})
reveal_type(S)  # revealed: TypeVar
```

### No explicit specialization

A type variable itself cannot be explicitly specialized; the result of the specialization is
`Unknown`. However, generic PEP 613 type aliases that point to type variables can be explicitly
specialized.

```py
from typing import TypeVar, TypeAlias

T = TypeVar("T")
ImplicitPositive = T
Positive: TypeAlias = T

def _(
    # error: [invalid-type-form] "A type variable itself cannot be specialized"
    a: T[int],
    # error: [invalid-type-form] "A type variable itself cannot be specialized"
    b: T[T],
    # error: [invalid-type-form] "A type variable itself cannot be specialized"
    c: ImplicitPositive[int],
    d: Positive[int],
):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: int
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
reveal_type(type(T))  # revealed: <class 'TypeVar'>
reveal_type(T)  # revealed: TypeVar
reveal_type(T.__default__)  # revealed: int
reveal_type(T.__bound__)  # revealed: None
reveal_type(T.__constraints__)  # revealed: tuple[()]

S = TypeVar("S")
reveal_type(S.__default__)  # revealed: NoDefault
```

### Using other typevars as a default

```toml
[environment]
python-version = "3.13"
```

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
reveal_type(type(T))  # revealed: <class 'TypeVar'>
reveal_type(T)  # revealed: TypeVar
reveal_type(T.__bound__)  # revealed: int
reveal_type(T.__constraints__)  # revealed: tuple[()]

S = TypeVar("S")
reveal_type(S.__bound__)  # revealed: None
```

The upper bound must be a valid type expression:

```py
from typing import TypedDict

# error: [invalid-type-form]
T = TypeVar("T", bound=TypedDict)
```

### Type variables with constraints

```py
from typing import TypeVar

T = TypeVar("T", int, str)
reveal_type(type(T))  # revealed: <class 'TypeVar'>
reveal_type(T)  # revealed: TypeVar
reveal_type(T.__constraints__)  # revealed: tuple[int, str]

S = TypeVar("S")
reveal_type(S.__constraints__)  # revealed: tuple[()]
```

Constraints are not simplified relative to each other, even if one is a subtype of the other:

```py
T = TypeVar("T", int, bool)
reveal_type(T.__constraints__)  # revealed: tuple[int, bool]

S = TypeVar("S", float, str)
reveal_type(S.__constraints__)  # revealed: tuple[int | float, str]
```

### Cannot have only one constraint

> `TypeVar` supports constraining parametric types to a fixed set of possible types...There should
> be at least two constraints, if any; specifying a single constraint is disallowed.

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", int)
```

### Cannot have both bound and constraint

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", int, str, bound=bytes)
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

### Boolean parameters must be unambiguous

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

### Invalid keyword arguments

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", invalid_keyword=True)
```

```pyi
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", invalid_keyword=True)
```

### Forward references in stubs

Stubs natively support forward references, so patterns that would raise `NameError` at runtime are
allowed in stub files:

`stub.pyi`:

```pyi
from typing import TypeVar

T = TypeVar("T", bound=A, default=B)
U = TypeVar("U", C, D)

class A: ...
class B(A): ...
class C: ...
class D: ...

def f(x: T) -> T: ...
def g(x: U) -> U: ...
```

`main.py`:

```py
from stub import f, g, A, B, C, D

reveal_type(f(A()))  # revealed: A
reveal_type(f(B()))  # revealed: B
reveal_type(g(C()))  # revealed: C
reveal_type(g(D()))  # revealed: D

# TODO: one diagnostic would probably be sufficient here...?
#
# error: [invalid-argument-type] "Argument type `C` does not satisfy upper bound `A` of type variable `T`"
# error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `B`, found `C`"
reveal_type(f(C()))  # revealed: B

# error: [invalid-argument-type]
reveal_type(g(A()))  # revealed: Unknown
```

### Constructor signature versioning

#### For `typing.TypeVar`

```toml
[environment]
python-version = "3.10"
```

In a stub file, features from the latest supported Python version can be used on any version.
There's no need to require use of `typing_extensions.TypeVar` in a stub file, when the type checker
can understand the typevar definition perfectly well either way, and there can be no runtime error.
(Perhaps it's arguable whether this special case is worth it, but other type checkers do it, so we
maintain compatibility.)

```pyi
from typing import TypeVar
T = TypeVar("T", default=int)
```

But this raises an error in a non-stub file:

```py
from typing import TypeVar

# error: [invalid-legacy-type-variable]
T = TypeVar("T", default=int)
```

#### For `typing_extensions.TypeVar`

`typing_extensions.TypeVar` always supports the latest features, on any Python version.

```toml
[environment]
python-version = "3.10"
```

```py
from typing_extensions import TypeVar

T = TypeVar("T", default=int)
# TODO: should not error, should reveal `int`
# error: [unresolved-attribute]
reveal_type(T.__default__)  # revealed: Unknown
```

## Callability

A typevar bound to a Callable type is callable:

```py
from typing import Callable, TypeVar

T = TypeVar("T", bound=Callable[[], int])

def bound(f: T):
    reveal_type(f)  # revealed: T@bound
    reveal_type(f())  # revealed: int
```

Same with a constrained typevar, as long as all constraints are callable:

```py
T = TypeVar("T", Callable[[], int], Callable[[], str])

def constrained(f: T):
    reveal_type(f)  # revealed: T@constrained
    reveal_type(f())  # revealed: int | str
```

## Meta-type

The meta-type of a typevar is `type[T]`.

```py
from typing import TypeVar

T_normal = TypeVar("T_normal")

def normal(x: T_normal):
    reveal_type(type(x))  # revealed: type[T_normal@normal]

T_bound_object = TypeVar("T_bound_object", bound=object)

def bound_object(x: T_bound_object):
    reveal_type(type(x))  # revealed: type[T_bound_object@bound_object]

T_bound_int = TypeVar("T_bound_int", bound=int)

def bound_int(x: T_bound_int):
    reveal_type(type(x))  # revealed: type[T_bound_int@bound_int]

T_constrained = TypeVar("T_constrained", int, str)

def constrained(x: T_constrained):
    reveal_type(type(x))  # revealed: type[T_constrained@constrained]
```

## Cycles

### Bounds and constraints

A typevar's bounds and constraints cannot be generic, cyclic or otherwise:

```py
from typing import Any, TypeVar

S = TypeVar("S")

# TODO: error
T = TypeVar("T", bound=list[S])

# TODO: error
U = TypeVar("U", list["T"], str)

# TODO: error
V = TypeVar("V", list["V"], str)
```

However, they are lazily evaluated and can cyclically refer to their own type:

```py
from typing import TypeVar, Generic

T = TypeVar("T", bound=list["G"])

class G(Generic[T]):
    x: T

reveal_type(G[list[G]]().x)  # revealed: list[G[Unknown]]
```

An invalid specialization in a recursive bound doesn't cause a panic:

```py
from typing import TypeVar, Generic

# error: [invalid-type-arguments]
T = TypeVar("T", bound="Node[int]")

class Node(Generic[T]):
    pass

# error: [invalid-type-arguments]
def _(n: Node[str]):
    reveal_type(n)  # revealed: Node[Unknown]
```

### Defaults

```toml
[environment]
python-version = "3.13"
```

Defaults can be generic, but can only refer to typevars from the same scope if they were defined
earlier in that scope:

```py
from typing import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U", default=T)

class C(Generic[T, U]):
    x: T
    y: U

reveal_type(C[int, str]().x)  # revealed: int
reveal_type(C[int, str]().y)  # revealed: str
reveal_type(C[int]().x)  # revealed: int
reveal_type(C[int]().y)  # revealed: int

# TODO: error
V = TypeVar("V", default="V")

class D(Generic[V]):
    x: V

reveal_type(D().x)  # revealed: Unknown
```

## Regression

### Use of typevar with default inside a function body that binds it

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Generic, TypeVar

_DataT = TypeVar("_DataT", bound=int, default=int)

class Event(Generic[_DataT]):
    def __init__(self, data: _DataT) -> None:
        self.data = data

def async_fire_internal(event_data: _DataT):
    event: Event[_DataT] | None = None
    event = Event(event_data)
```

[generics]: https://typing.python.org/en/latest/spec/generics.html
