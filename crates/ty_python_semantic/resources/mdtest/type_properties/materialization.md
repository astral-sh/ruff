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
from ty_extensions import Unknown, Top

def _(top_any: Top[Any], top_unknown: Top[Unknown]):
    reveal_type(top_any)  # revealed: object
    reveal_type(top_unknown)  # revealed: object
```

The contravariant position is replaced with `Never`.

```py
def _(top_callable: Top[Callable[[Any], None]]):
    reveal_type(top_callable)  # revealed: (Never, /) -> None
```

The invariant position is replaced with an unresolved type variable.

```py
def _(top_list: Top[list[Any]]):
    reveal_type(top_list)  # revealed: list[T_all]
```

### Bottom materialization

The dynamic type at the top-level is replaced with `Never`.

```py
from typing import Any, Callable
from ty_extensions import Unknown, Bottom

def _(bottom_any: Bottom[Any], bottom_unknown: Bottom[Unknown]):
    reveal_type(bottom_any)  # revealed: Never
    reveal_type(bottom_unknown)  # revealed: Never
```

The contravariant position is replaced with `object`.

```py
def _(bottom_callable: Bottom[Callable[[Any, Unknown], None]]):
    reveal_type(bottom_callable)  # revealed: (object, object, /) -> None
```

The invariant position is replaced in the same way as the top materialization, with an unresolved
type variable.

```py
def _(bottom_list: Bottom[list[Any]]):
    reveal_type(bottom_list)  # revealed: list[T_all]
```

## Fully static types

The top / bottom (and only) materialization of any fully static type is just itself.

```py
from typing import Any, Literal
from ty_extensions import TypeOf, Bottom, Top, is_equivalent_to, static_assert
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

static_assert(is_equivalent_to(Top[int], int))
static_assert(is_equivalent_to(Bottom[int], int))

static_assert(is_equivalent_to(Top[Literal[1]], Literal[1]))
static_assert(is_equivalent_to(Bottom[Literal[1]], Literal[1]))

static_assert(is_equivalent_to(Top[Literal[True]], Literal[True]))
static_assert(is_equivalent_to(Bottom[Literal[True]], Literal[True]))

static_assert(is_equivalent_to(Top[Literal["abc"]], Literal["abc"]))
static_assert(is_equivalent_to(Bottom[Literal["abc"]], Literal["abc"]))

static_assert(is_equivalent_to(Top[Literal[Answer.YES]], Literal[Answer.YES]))
static_assert(is_equivalent_to(Bottom[Literal[Answer.YES]], Literal[Answer.YES]))

static_assert(is_equivalent_to(Top[int | str], int | str))
static_assert(is_equivalent_to(Bottom[int | str], int | str))
```

We currently treat function literals as fully static types, so they remain unchanged even though the
signature might have `Any` in it. (TODO: this is probably not right.)

```py
def function(x: Any) -> None: ...

class A:
    def method(self, x: Any) -> None: ...

def _(
    top_func: Top[TypeOf[function]],
    bottom_func: Bottom[TypeOf[function]],
    top_meth: Top[TypeOf[A().method]],
    bottom_meth: Bottom[TypeOf[A().method]],
):
    reveal_type(top_func)  # revealed: def function(x: Any) -> None
    reveal_type(bottom_func)  # revealed: def function(x: Any) -> None

    reveal_type(top_meth)  # revealed: bound method A.method(x: Any) -> None
    reveal_type(bottom_meth)  # revealed: bound method A.method(x: Any) -> None
```

## Callable

For a callable, the parameter types are in a contravariant position, and the return type is in a
covariant position.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Callable
from ty_extensions import TypeOf, Unknown, Bottom, Top

type C1 = Callable[[Any, Unknown], Any]

def _(top: Top[C1], bottom: Bottom[C1]) -> None:
    reveal_type(top)  # revealed: (Never, Never, /) -> object
    reveal_type(bottom)  # revealed: (object, object, /) -> Never
```

The parameter types in a callable inherits the contravariant position.

```py
type C2 = Callable[[int, tuple[int | Any]], tuple[Any]]

def _(top: Top[C2], bottom: Bottom[C2]) -> None:
    reveal_type(top)  # revealed: (int, tuple[int], /) -> tuple[object]
    reveal_type(bottom)  # revealed: (int, tuple[object], /) -> Never
```

But, if the callable itself is in a contravariant position, then the variance is flipped i.e., if
the outer variance is covariant, it's flipped to contravariant, and if it's contravariant, it's
flipped to covariant, invariant remains invariant.

```py
type C3 = Callable[[Any, Callable[[Unknown], Any]], Callable[[Any, int], Any]]

def _(top: Top[C3], bottom: Bottom[C3]) -> None:
    # revealed: (Never, (object, /) -> Never, /) -> (Never, int, /) -> object
    reveal_type(top)

    # revealed: (object, (Never, /) -> object, /) -> (object, int, /) -> Never
    reveal_type(bottom)
```

## Tuple

All positions in a tuple are covariant.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Never
from ty_extensions import Unknown, Bottom, Top, is_equivalent_to, static_assert

static_assert(is_equivalent_to(Top[tuple[Any, int]], tuple[object, int]))
static_assert(is_equivalent_to(Bottom[tuple[Any, int]], Never))

static_assert(is_equivalent_to(Top[tuple[Unknown, int]], tuple[object, int]))
static_assert(is_equivalent_to(Bottom[tuple[Unknown, int]], Never))

static_assert(is_equivalent_to(Top[tuple[Any, int, Unknown]], tuple[object, int, object]))
static_assert(is_equivalent_to(Bottom[tuple[Any, int, Unknown]], Never))
```

Except for when the tuple itself is in a contravariant position, then all positions in the tuple
inherit the contravariant position.

```py
from typing import Callable
from ty_extensions import TypeOf

type C = Callable[[tuple[Any, int], tuple[str, Unknown]], None]

def _(top: Top[C], bottom: Bottom[C]) -> None:
    reveal_type(top)  # revealed: (Never, Never, /) -> None
    reveal_type(bottom)  # revealed: (tuple[object, int], tuple[str, object], /) -> None
```

And, similarly for an invariant position.

```py
type LTAnyInt = list[tuple[Any, int]]
type LTStrUnknown = list[tuple[str, Unknown]]
type LTAnyIntUnknown = list[tuple[Any, int, Unknown]]

def _(
    top_ai: Top[LTAnyInt],
    bottom_ai: Bottom[LTAnyInt],
    top_su: Top[LTStrUnknown],
    bottom_su: Bottom[LTStrUnknown],
    top_aiu: Top[LTAnyIntUnknown],
    bottom_aiu: Bottom[LTAnyIntUnknown],
):
    reveal_type(top_ai)  # revealed: list[tuple[T_all, int]]
    reveal_type(bottom_ai)  # revealed: list[tuple[T_all, int]]

    reveal_type(top_su)  # revealed: list[tuple[str, T_all]]
    reveal_type(bottom_su)  # revealed: list[tuple[str, T_all]]

    reveal_type(top_aiu)  # revealed: list[tuple[T_all, int, T_all]]
    reveal_type(bottom_aiu)  # revealed: list[tuple[T_all, int, T_all]]
```

## Union

All positions in a union are covariant.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from ty_extensions import Unknown, Bottom, Top, static_assert, is_equivalent_to

static_assert(is_equivalent_to(Top[Any | int], object))
static_assert(is_equivalent_to(Bottom[Any | int], int))

static_assert(is_equivalent_to(Top[Unknown | int], object))
static_assert(is_equivalent_to(Bottom[Unknown | int], int))

static_assert(is_equivalent_to(Top[int | str | Any], object))
static_assert(is_equivalent_to(Bottom[int | str | Any], int | str))
```

Except for when the union itself is in a contravariant position, then all positions in the union
inherit the contravariant position.

```py
from typing import Callable
from ty_extensions import TypeOf

def _(callable: Callable[[Any | int, str | Unknown], None]) -> None:
    static_assert(is_equivalent_to(Top[TypeOf[callable]], Callable[[int, str], None]))
    static_assert(is_equivalent_to(Bottom[TypeOf[callable]], Callable[[object, object], None]))
```

And, similarly for an invariant position.

```py
def _(
    top_ai: Top[list[Any | int]],
    bottom_ai: Bottom[list[Any | int]],
    top_su: Top[list[str | Unknown]],
    bottom_su: Bottom[list[str | Unknown]],
    top_aiu: Top[list[Any | int | Unknown]],
    bottom_aiu: Bottom[list[Any | int | Unknown]],
):
    reveal_type(top_ai)  # revealed: list[T_all | int]
    reveal_type(bottom_ai)  # revealed: list[T_all | int]

    reveal_type(top_su)  # revealed: list[str | T_all]
    reveal_type(bottom_su)  # revealed: list[str | T_all]

    reveal_type(top_aiu)  # revealed: list[T_all | int]
    reveal_type(bottom_aiu)  # revealed: list[T_all | int]
```

## Intersection

All positions in an intersection are covariant.

```py
from typing import Any
from typing_extensions import Never
from ty_extensions import Intersection, Unknown, Bottom, Top, static_assert, is_equivalent_to

static_assert(is_equivalent_to(Top[Intersection[Any, int]], int))
static_assert(is_equivalent_to(Bottom[Intersection[Any, int]], Never))

# Here, the top materialization of `Any | int` is `object` and the intersection of it with tuple
static_assert(is_equivalent_to(Top[Intersection[Any | int, tuple[str, Unknown]]], tuple[str, object]))
static_assert(is_equivalent_to(Bottom[Intersection[Any | int, tuple[str, Unknown]]], Never))

class Foo: ...

static_assert(is_equivalent_to(Bottom[Intersection[Any | Foo, tuple[str]]], Intersection[Foo, tuple[str]]))

def _(
    top: Top[Intersection[list[Any], list[int]]],
    bottom: Bottom[Intersection[list[Any], list[int]]],
):
    reveal_type(top)  # revealed: list[T_all] & list[int]
    reveal_type(bottom)  # revealed: list[T_all] & list[int]
```

## Negation (via `Not`)

All positions in a negation are contravariant.

```py
from typing import Any
from typing_extensions import Never
from ty_extensions import Not, Unknown, Bottom, Top, static_assert, is_equivalent_to

# ~Any is still Any, so the top materialization is object
static_assert(is_equivalent_to(Top[Not[Any]], object))
static_assert(is_equivalent_to(Bottom[Not[Any]], Never))

# tuple[Any, int] is in a contravariant position, so the
# top materialization is Never and the negation of it
static_assert(is_equivalent_to(Top[Not[tuple[Any, int]]], object))
static_assert(is_equivalent_to(Bottom[Not[tuple[Any, int]]], Not[tuple[object, int]]))
```

## `type`

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from typing_extensions import Never
from ty_extensions import Unknown, Bottom, Top, static_assert, is_equivalent_to

static_assert(is_equivalent_to(Top[type[Any]], type))
static_assert(is_equivalent_to(Bottom[type[Any]], Never))

static_assert(is_equivalent_to(Top[type[Unknown]], type))
static_assert(is_equivalent_to(Bottom[type[Unknown]], Never))

static_assert(is_equivalent_to(Top[type[int | Any]], type))
static_assert(is_equivalent_to(Bottom[type[int | Any]], type[int]))

# Here, `T` has an upper bound of `type`
def _(top: Top[list[type[Any]]], bottom: Bottom[list[type[Any]]]):
    reveal_type(top)  # revealed: list[T_all]
    reveal_type(bottom)  # revealed: list[T_all]
```

## Type variables

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Never, TypeVar
from ty_extensions import Unknown, Bottom, Top, static_assert, is_subtype_of

def bounded_by_gradual[T: Any](t: T) -> None:
    # Top materialization of `T: Any` is `T: object`

    # Bottom materialization of `T: Any` is `T: Never`
    static_assert(is_subtype_of(Bottom[T], Never))

def constrained_by_gradual[T: (int, Any)](t: T) -> None:
    # Top materialization of `T: (int, Any)` is `T: (int, object)`

    # Bottom materialization of `T: (int, Any)` is `T: (int, Never)`
    static_assert(is_subtype_of(Bottom[T], int))
```

## Generics

For generics, the materialization depends on the surrounding variance and the variance of the type
variable itself.

- If the type variable is invariant, the materialization happens in an invariant position
- If the type variable is covariant, the materialization happens as per the surrounding variance
- If the type variable is contravariant, the materialization happens as per the surrounding
    variance, but the variance is flipped

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Generic, TypeVar, Never
from ty_extensions import Bottom, Top, static_assert, is_equivalent_to

T = TypeVar("T")
T_co = TypeVar("T_co", covariant=True)
T_contra = TypeVar("T_contra", contravariant=True)

class GenericInvariant(Generic[T]):
    pass

class GenericCovariant(Generic[T_co]):
    pass

class GenericContravariant(Generic[T_contra]):
    pass

def _(top: Top[GenericInvariant[Any]], bottom: Bottom[GenericInvariant[Any]]):
    reveal_type(top)  # revealed: GenericInvariant[T_all]
    reveal_type(bottom)  # revealed: GenericInvariant[T_all]

static_assert(is_equivalent_to(Top[GenericCovariant[Any]], GenericCovariant[object]))
static_assert(is_equivalent_to(Bottom[GenericCovariant[Any]], GenericCovariant[Never]))

static_assert(is_equivalent_to(Top[GenericContravariant[Any]], GenericContravariant[Never]))
static_assert(is_equivalent_to(Bottom[GenericContravariant[Any]], GenericContravariant[object]))
```

Parameters in callable are contravariant, so the variance should be flipped:

```py
from typing import Callable
from ty_extensions import TypeOf

type InvariantCallable = Callable[[GenericInvariant[Any]], None]
type CovariantCallable = Callable[[GenericCovariant[Any]], None]
type ContravariantCallable = Callable[[GenericContravariant[Any]], None]

def invariant(top: Top[InvariantCallable], bottom: Bottom[InvariantCallable]) -> None:
    reveal_type(top)  # revealed: (GenericInvariant[T_all], /) -> None
    reveal_type(bottom)  # revealed: (GenericInvariant[T_all], /) -> None

def covariant(top: Top[CovariantCallable], bottom: Bottom[CovariantCallable]) -> None:
    reveal_type(top)  # revealed: (GenericCovariant[Never], /) -> None
    reveal_type(bottom)  # revealed: (GenericCovariant[object], /) -> None

def contravariant(top: Top[ContravariantCallable], bottom: Bottom[ContravariantCallable]) -> None:
    reveal_type(top)  # revealed: (GenericContravariant[object], /) -> None
    reveal_type(bottom)  # revealed: (GenericContravariant[Never], /) -> None
```

## Invalid use

`Top[]` and `Bottom[]` are special forms that take a single argument.

It is invalid to use them without a type argument.

```py
from ty_extensions import Bottom, Top

def _(
    just_top: Top,  # error: [invalid-type-form]
    just_bottom: Bottom,  # error: [invalid-type-form]
): ...
```

It is also invalid to use multiple arguments:

```py
def _(
    top_two: Top[int, str],  # error: [invalid-type-form]
    bottom_two: Bottom[int, str],  # error: [invalid-type-form]
): ...
```

The argument must be a type expression:

```py
def _(
    top_1: Top[1],  # error: [invalid-type-form]
    bottom_1: Bottom[1],  # error: [invalid-type-form]
): ...
```
