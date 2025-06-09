# Top materialization

The top materialization or upper bound materialization of a type is the most general form of that
type which is fully static.

More concretely, `T'`, the top materialization of `T`, is the type `T` with all occurrences of `Any`
and `Unknown` replaced as follows:

- In covariant position, it's replaced with `object`
- In contravariant position, it's replaced with `Never`
- In invariant position, it's replaced with an unresolved type variable

For an invariant position, it should actually be replaced with a `forall T. list[T]`, but this is
not representable in our type system, so we use an unresolved type variable instead.

## Replacement rules

```py
from typing import Any, Callable
from ty_extensions import Unknown, top_materialization

# Covariant position
reveal_type(top_materialization(Any))  # revealed: object
reveal_type(top_materialization(Unknown))  # revealed: object

# Contravariant position
reveal_type(top_materialization(Callable[[Any], None]))  # revealed: (Never, /) -> None

# Invariant position
reveal_type(top_materialization(list[Any]))  # revealed: list[T]
```

## Fully static types

The top materialization is mainly useful for gradual types, so any fully static type would remain
unchanged.

```py
from typing import Any, Literal
from ty_extensions import TypeOf, top_materialization

reveal_type(top_materialization(int))  # revealed: int
reveal_type(top_materialization(Literal[1]))  # revealed: Literal[1]
reveal_type(top_materialization(Literal[True]))  # revealed: Literal[True]
reveal_type(top_materialization(Literal["abc"]))  # revealed: Literal["abc"]
reveal_type(top_materialization(int | str))  # revealed: int | str
```

Function literals are fully static types, so they remain unchanged even though the signature might
have `Any` in it.

```py
def function(x: Any) -> None: ...

class A:
    def method(self, x: Any) -> None: ...

reveal_type(top_materialization(TypeOf[function]))  # revealed: def function(x: Any) -> None
reveal_type(top_materialization(TypeOf[A().method]))  # revealed: bound method A.method(x: Any) -> None
```

## Tuple

All positions in a tuple are covariant.

```py
from typing import Any
from ty_extensions import Unknown, top_materialization

reveal_type(top_materialization(tuple[Any, int]))  # revealed: tuple[object, int]
reveal_type(top_materialization(tuple[Unknown, int]))  # revealed: tuple[object, int]
reveal_type(top_materialization(tuple[Any, int, Unknown]))  # revealed: tuple[object, int, object]
```

Except for when the tuple itself is in a contravariant position, then all positions in the tuple
inherit the contravariant position.

```py
from typing import Callable

# revealed: (Never, Never, /) -> None
reveal_type(top_materialization(Callable[[tuple[Any, int], tuple[str, Unknown]], None]))
```

And, similarly for an invariant position.

```py
reveal_type(top_materialization(list[tuple[Any, int]]))  # revealed: list[tuple[T, int]]
reveal_type(top_materialization(list[tuple[str, Unknown]]))  # revealed: list[tuple[str, T]]
reveal_type(top_materialization(list[tuple[Any, int, Unknown]]))  # revealed: list[tuple[T, int, T]]
```

## Union

All positions in a union are covariant.

```py
from typing import Any
from ty_extensions import Unknown, top_materialization

reveal_type(top_materialization(Any | int))  # revealed: object
reveal_type(top_materialization(Unknown | int))  # revealed: object
reveal_type(top_materialization(int | str | Any))  # revealed: object
```

Except for when the union itself is in a contravariant position, then all positions in the union
inherit the contravariant position.

```py
from typing import Callable

# revealed: (int, str, /) -> None
reveal_type(top_materialization(Callable[[Any | int, str | Unknown], None]))
```

And, similarly for an invariant position.

```py
reveal_type(top_materialization(list[Any | int]))  # revealed: list[T | int]
reveal_type(top_materialization(list[str | Unknown]))  # revealed: list[str | T]
reveal_type(top_materialization(list[Any | int | Unknown]))  # revealed: list[T | int]
```

## Intersection

All positions in an intersection are covariant.

```py
from typing import Any
from ty_extensions import Intersection, Unknown, top_materialization

reveal_type(top_materialization(Intersection[Any, int]))  # revealed: int

# Here, the top materialization of `Any | int` is `object` and the intersection of it with tuple
# revealed: tuple[str, object]
reveal_type(top_materialization(Intersection[Any | int, tuple[str, Unknown]]))

reveal_type(top_materialization(Intersection[list[Any], list[int]]))  # revealed: list[T] & list[int]
```

## Negation (via `Not`)

All positions in a negation are contravariant.

```py
from typing import Any
from ty_extensions import Not, Unknown, top_materialization

# ~Any is still Any, so the top materialization is object
reveal_type(top_materialization(Not[Any]))  # revealed: object

# tuple[Any, int] is in a contravariant position, so the
# top materialization is Never and the negation of it
# revealed: object
reveal_type(top_materialization(Not[tuple[Any, int]]))
```

## `type`

```py
from typing import Any
from ty_extensions import Unknown, top_materialization

reveal_type(top_materialization(type[Any]))  # revealed: type
reveal_type(top_materialization(type[Unknown]))  # revealed: type
reveal_type(top_materialization(type[int | Any]))  # revealed: type
```

## Type variables

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, TypeVar
from ty_extensions import TypeOf, Unknown, top_materialization, is_fully_static, static_assert

def bounded_by_gradual[T: Any](t: T) -> None:
    static_assert(not is_fully_static(T))
    static_assert(is_fully_static(TypeOf[top_materialization(T)]))

def constrained_by_gradual[T: (int, Any)](t: T) -> None:
    static_assert(not is_fully_static(T))
    static_assert(is_fully_static(TypeOf[top_materialization(T)]))
```

## Generics

For generics, the top materialization depends on whether the type variable is covariant or
contravariant.

```py
from typing import Any, Generic, TypeVar
from ty_extensions import top_materialization

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)
T_contra = TypeVar("T_contra", contravariant=True)

class GenericInvariant(Generic[T]):
    pass

class GenericCovariant(Generic[T_co]):
    pass

class GenericContravariant(Generic[T_contra]):
    pass

reveal_type(top_materialization(GenericInvariant[Any]))  # revealed: GenericInvariant[T]
reveal_type(top_materialization(GenericCovariant[Any]))  # revealed: GenericCovariant[object]
reveal_type(top_materialization(GenericContravariant[Any]))  # revealed: GenericContravariant[Never]
```

## Callable

For a callable, the parameter types are in a contravariant position, and the return type is in a
covariant position.

```py
from typing import Any, Callable
from ty_extensions import Unknown, top_materialization

reveal_type(top_materialization(Callable[[Any, Unknown], Any]))  # revealed: (Never, Never, /) -> object
```

The parameter types in a callable inherits the contravariant position.

```py
# revealed: (int, tuple[int], (Never, /) -> Never, /) -> tuple[object, (Never, /) -> object]
reveal_type(top_materialization(Callable[[int, tuple[int | Any], Callable[[Unknown], Any]], tuple[Any, Callable[[Any], Any]]]))
```
