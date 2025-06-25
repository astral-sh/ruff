# Materialization

There are two materializations of a type:

- The top materialization (or upper bound materialization) of a type, which is the most general form
    of that type that is fully static
- The bottom materialization (or lower bound materialization) of a type, which is the most specific
    form of that type that is fully static

More concretely, `T'`, the materialization of `T`, is the type `T` with all occurrences of `Any` and
`Unknown` replaced as follows:

- In covariant position, it's replaced with `object`
- In contravariant position, it's replaced with `Never`
- In invariant position, it's replaced with an unresolved type variable

The top materialization starts from the covariant position while the bottom materialization starts
from the contravariant position.

TODO: For an invariant position, e.g. `list[Any]`, it should be replaced with an existential type
representing "all lists, containing any type". We currently represent this by replacing `Any` in
invariant position with an unresolved type variable.

## Replacement rules

### Top materialization

The dynamic type at the top-level is replaced with `object`.

```py
from typing import Any, Callable
from ty_extensions import Unknown, top_materialization

reveal_type(top_materialization(Any))  # revealed: object
reveal_type(top_materialization(Unknown))  # revealed: object
```

The contravariant position is replaced with `Never`.

```py
reveal_type(top_materialization(Callable[[Any], None]))  # revealed: (Never, /) -> None
```

The invariant position is replaced with an unresolved type variable.

```py
reveal_type(top_materialization(list[Any]))  # revealed: list[T_all]
```

### Bottom materialization

The dynamic type at the top-level is replaced with `Never`.

```py
from typing import Any, Callable
from ty_extensions import Unknown, bottom_materialization

reveal_type(bottom_materialization(Any))  # revealed: Never
reveal_type(bottom_materialization(Unknown))  # revealed: Never
```

The contravariant position is replaced with `object`.

```py
# revealed: (object, object, /) -> None
reveal_type(bottom_materialization(Callable[[Any, Unknown], None]))
```

The invariant position is replaced in the same way as the top materialization, with an unresolved
type variable.

```py
reveal_type(bottom_materialization(list[Any]))  # revealed: list[T_all]
```

## Fully static types

The top / bottom (and only) materialization of any fully static type is just itself.

```py
from typing import Any, Literal
from ty_extensions import TypeOf, bottom_materialization, top_materialization

reveal_type(top_materialization(int))  # revealed: int
reveal_type(bottom_materialization(int))  # revealed: int

reveal_type(top_materialization(Literal[1]))  # revealed: Literal[1]
reveal_type(bottom_materialization(Literal[1]))  # revealed: Literal[1]

reveal_type(top_materialization(Literal[True]))  # revealed: Literal[True]
reveal_type(bottom_materialization(Literal[True]))  # revealed: Literal[True]

reveal_type(top_materialization(Literal["abc"]))  # revealed: Literal["abc"]
reveal_type(bottom_materialization(Literal["abc"]))  # revealed: Literal["abc"]

reveal_type(top_materialization(int | str))  # revealed: int | str
reveal_type(bottom_materialization(int | str))  # revealed: int | str
```

We currently treat function literals as fully static types, so they remain unchanged even though the
signature might have `Any` in it. (TODO: this is probably not right.)

```py
def function(x: Any) -> None: ...

class A:
    def method(self, x: Any) -> None: ...

reveal_type(top_materialization(TypeOf[function]))  # revealed: def function(x: Any) -> None
reveal_type(bottom_materialization(TypeOf[function]))  # revealed: def function(x: Any) -> None

reveal_type(top_materialization(TypeOf[A().method]))  # revealed: bound method A.method(x: Any) -> None
reveal_type(bottom_materialization(TypeOf[A().method]))  # revealed: bound method A.method(x: Any) -> None
```

## Callable

For a callable, the parameter types are in a contravariant position, and the return type is in a
covariant position.

```py
from typing import Any, Callable
from ty_extensions import TypeOf, Unknown, bottom_materialization, top_materialization

def _(callable: Callable[[Any, Unknown], Any]) -> None:
    # revealed: (Never, Never, /) -> object
    reveal_type(top_materialization(TypeOf[callable]))

    # revealed: (object, object, /) -> Never
    reveal_type(bottom_materialization(TypeOf[callable]))
```

The parameter types in a callable inherits the contravariant position.

```py
def _(callable: Callable[[int, tuple[int | Any]], tuple[Any]]) -> None:
    # revealed: (int, tuple[int], /) -> tuple[object]
    reveal_type(top_materialization(TypeOf[callable]))

    # revealed: (int, tuple[object], /) -> Never
    reveal_type(bottom_materialization(TypeOf[callable]))
```

But, if the callable itself is in a contravariant position, then the variance is flipped i.e., if
the outer variance is covariant, it's flipped to contravariant, and if it's contravariant, it's
flipped to covariant, invariant remains invariant.

```py
def _(callable: Callable[[Any, Callable[[Unknown], Any]], Callable[[Any, int], Any]]) -> None:
    # revealed: (Never, (object, /) -> Never, /) -> (Never, int, /) -> object
    reveal_type(top_materialization(TypeOf[callable]))

    # revealed: (object, (Never, /) -> object, /) -> (object, int, /) -> Never
    reveal_type(bottom_materialization(TypeOf[callable]))
```

## Tuple

All positions in a tuple are covariant.

```py
from typing import Any
from ty_extensions import Unknown, bottom_materialization, top_materialization

reveal_type(top_materialization(tuple[Any, int]))  # revealed: tuple[object, int]
reveal_type(bottom_materialization(tuple[Any, int]))  # revealed: Never

reveal_type(top_materialization(tuple[Unknown, int]))  # revealed: tuple[object, int]
reveal_type(bottom_materialization(tuple[Unknown, int]))  # revealed: Never

reveal_type(top_materialization(tuple[Any, int, Unknown]))  # revealed: tuple[object, int, object]
reveal_type(bottom_materialization(tuple[Any, int, Unknown]))  # revealed: Never
```

Except for when the tuple itself is in a contravariant position, then all positions in the tuple
inherit the contravariant position.

```py
from typing import Callable
from ty_extensions import TypeOf

def _(callable: Callable[[tuple[Any, int], tuple[str, Unknown]], None]) -> None:
    # revealed: (Never, Never, /) -> None
    reveal_type(top_materialization(TypeOf[callable]))

    # revealed: (tuple[object, int], tuple[str, object], /) -> None
    reveal_type(bottom_materialization(TypeOf[callable]))
```

And, similarly for an invariant position.

```py
reveal_type(top_materialization(list[tuple[Any, int]]))  # revealed: list[tuple[T_all, int]]
reveal_type(bottom_materialization(list[tuple[Any, int]]))  # revealed: list[tuple[T_all, int]]

reveal_type(top_materialization(list[tuple[str, Unknown]]))  # revealed: list[tuple[str, T_all]]
reveal_type(bottom_materialization(list[tuple[str, Unknown]]))  # revealed: list[tuple[str, T_all]]

reveal_type(top_materialization(list[tuple[Any, int, Unknown]]))  # revealed: list[tuple[T_all, int, T_all]]
reveal_type(bottom_materialization(list[tuple[Any, int, Unknown]]))  # revealed: list[tuple[T_all, int, T_all]]
```

## Union

All positions in a union are covariant.

```py
from typing import Any
from ty_extensions import Unknown, bottom_materialization, top_materialization

reveal_type(top_materialization(Any | int))  # revealed: object
reveal_type(bottom_materialization(Any | int))  # revealed: int

reveal_type(top_materialization(Unknown | int))  # revealed: object
reveal_type(bottom_materialization(Unknown | int))  # revealed: int

reveal_type(top_materialization(int | str | Any))  # revealed: object
reveal_type(bottom_materialization(int | str | Any))  # revealed: int | str
```

Except for when the union itself is in a contravariant position, then all positions in the union
inherit the contravariant position.

```py
from typing import Callable
from ty_extensions import TypeOf

def _(callable: Callable[[Any | int, str | Unknown], None]) -> None:
    # revealed: (int, str, /) -> None
    reveal_type(top_materialization(TypeOf[callable]))

    # revealed: (object, object, /) -> None
    reveal_type(bottom_materialization(TypeOf[callable]))
```

And, similarly for an invariant position.

```py
reveal_type(top_materialization(list[Any | int]))  # revealed: list[T_all | int]
reveal_type(bottom_materialization(list[Any | int]))  # revealed: list[T_all | int]

reveal_type(top_materialization(list[str | Unknown]))  # revealed: list[str | T_all]
reveal_type(bottom_materialization(list[str | Unknown]))  # revealed: list[str | T_all]

reveal_type(top_materialization(list[Any | int | Unknown]))  # revealed: list[T_all | int]
reveal_type(bottom_materialization(list[Any | int | Unknown]))  # revealed: list[T_all | int]
```

## Intersection

All positions in an intersection are covariant.

```py
from typing import Any
from ty_extensions import Intersection, Unknown, bottom_materialization, top_materialization

reveal_type(top_materialization(Intersection[Any, int]))  # revealed: int
reveal_type(bottom_materialization(Intersection[Any, int]))  # revealed: Never

# Here, the top materialization of `Any | int` is `object` and the intersection of it with tuple
# revealed: tuple[str, object]
reveal_type(top_materialization(Intersection[Any | int, tuple[str, Unknown]]))
# revealed: Never
reveal_type(bottom_materialization(Intersection[Any | int, tuple[str, Unknown]]))

class Foo: ...

# revealed: Foo & tuple[str]
reveal_type(bottom_materialization(Intersection[Any | Foo, tuple[str]]))

reveal_type(top_materialization(Intersection[list[Any], list[int]]))  # revealed: list[T_all] & list[int]
reveal_type(bottom_materialization(Intersection[list[Any], list[int]]))  # revealed: list[T_all] & list[int]
```

## Negation (via `Not`)

All positions in a negation are contravariant.

```py
from typing import Any
from ty_extensions import Not, Unknown, bottom_materialization, top_materialization

# ~Any is still Any, so the top materialization is object
reveal_type(top_materialization(Not[Any]))  # revealed: object
reveal_type(bottom_materialization(Not[Any]))  # revealed: Never

# tuple[Any, int] is in a contravariant position, so the
# top materialization is Never and the negation of it
# revealed: object
reveal_type(top_materialization(Not[tuple[Any, int]]))
# revealed: ~tuple[object, int]
reveal_type(bottom_materialization(Not[tuple[Any, int]]))
```

## `type`

```py
from typing import Any
from ty_extensions import Unknown, bottom_materialization, top_materialization

reveal_type(top_materialization(type[Any]))  # revealed: type
reveal_type(bottom_materialization(type[Any]))  # revealed: Never

reveal_type(top_materialization(type[Unknown]))  # revealed: type
reveal_type(bottom_materialization(type[Unknown]))  # revealed: Never

reveal_type(top_materialization(type[int | Any]))  # revealed: type
reveal_type(bottom_materialization(type[int | Any]))  # revealed: type[int]

# Here, `T` has an upper bound of `type`
reveal_type(top_materialization(list[type[Any]]))  # revealed: list[T_all]
reveal_type(bottom_materialization(list[type[Any]]))  # revealed: list[T_all]
```

## Type variables

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Never, TypeVar
from ty_extensions import (
    TypeOf,
    Unknown,
    bottom_materialization,
    top_materialization,
    static_assert,
    is_subtype_of,
)

def bounded_by_gradual[T: Any](t: T) -> None:
    # Top materialization of `T: Any` is `T: object`

    # Bottom materialization of `T: Any` is `T: Never`
    static_assert(is_subtype_of(TypeOf[bottom_materialization(T)], Never))

def constrained_by_gradual[T: (int, Any)](t: T) -> None:
    # Top materialization of `T: (int, Any)` is `T: (int, object)`

    # Bottom materialization of `T: (int, Any)` is `T: (int, Never)`
    static_assert(is_subtype_of(TypeOf[bottom_materialization(T)], int))
```

## Generics

For generics, the materialization depends on the surrounding variance and the variance of the type
variable itself.

- If the type variable is invariant, the materialization happens in an invariant position
- If the type variable is covariant, the materialization happens as per the surrounding variance
- If the type variable is contravariant, the materialization happens as per the surrounding
    variance, but the variance is flipped

```py
from typing import Any, Generic, TypeVar
from ty_extensions import bottom_materialization, top_materialization

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)
T_contra = TypeVar("T_contra", contravariant=True)

class GenericInvariant(Generic[T]):
    pass

class GenericCovariant(Generic[T_co]):
    pass

class GenericContravariant(Generic[T_contra]):
    pass

reveal_type(top_materialization(GenericInvariant[Any]))  # revealed: GenericInvariant[T_all]
reveal_type(bottom_materialization(GenericInvariant[Any]))  # revealed: GenericInvariant[T_all]

reveal_type(top_materialization(GenericCovariant[Any]))  # revealed: GenericCovariant[object]
reveal_type(bottom_materialization(GenericCovariant[Any]))  # revealed: GenericCovariant[Never]

reveal_type(top_materialization(GenericContravariant[Any]))  # revealed: GenericContravariant[Never]
reveal_type(bottom_materialization(GenericContravariant[Any]))  # revealed: GenericContravariant[object]
```

Parameters in callable are contravariant, so the variance should be flipped:

```py
from typing import Callable
from ty_extensions import TypeOf

def invariant(callable: Callable[[GenericInvariant[Any]], None]) -> None:
    # revealed: (GenericInvariant[T_all], /) -> None
    reveal_type(top_materialization(TypeOf[callable]))

    # revealed: (GenericInvariant[T_all], /) -> None
    reveal_type(bottom_materialization(TypeOf[callable]))

def covariant(callable: Callable[[GenericCovariant[Any]], None]) -> None:
    # revealed: (GenericCovariant[Never], /) -> None
    reveal_type(top_materialization(TypeOf[callable]))

    # revealed: (GenericCovariant[object], /) -> None
    reveal_type(bottom_materialization(TypeOf[callable]))

def contravariant(callable: Callable[[GenericContravariant[Any]], None]) -> None:
    # revealed: (GenericContravariant[object], /) -> None
    reveal_type(top_materialization(TypeOf[callable]))

    # revealed: (GenericContravariant[Never], /) -> None
    reveal_type(bottom_materialization(TypeOf[callable]))
```
