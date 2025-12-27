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

The invariant position cannot simplify, and is represented with the `Top` special form.

```py
def _(top_list: Top[list[Any]]):
    reveal_type(top_list)  # revealed: Top[list[Any]]
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

The invariant position is represented with the `Bottom` special form.

There is an argument that `Bottom[list[Any]]` should simplify to `Never`, since it is the infinite
intersection of all possible materializations of `list[Any]`, and (due to invariance) these
materializations are disjoint types. But currently we do not make this simplification: there doesn't
seem to be any compelling need for it, and allowing more gradual types to materialize to `Never` has
undesirable implications for mutual assignability of seemingly-unrelated gradual types.

```py
def _(bottom_list: Bottom[list[Any]]):
    reveal_type(bottom_list)  # revealed: Bottom[list[Any]]
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
    reveal_type(top_func)  # revealed: def function(x: Never) -> None
    reveal_type(bottom_func)  # revealed: def function(x: object) -> None

    reveal_type(top_meth)  # revealed: bound method A.method(x: Never) -> None
    reveal_type(bottom_meth)  # revealed: bound method A.method(x: object) -> None
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

## Callable with gradual parameters

For callables with gradual parameters (the `...` form), the top materialization preserves the
gradual form since we cannot know what parameters are required. The bottom materialization
simplifies to the bottom parameters `(*args: object, **kwargs: object)` since this is the most
specific type that is a subtype of all possible parameter materializations.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Callable, Never, Protocol
from ty_extensions import Bottom, Top, is_equivalent_to, is_subtype_of, static_assert

type GradualCallable = Callable[..., Any]

def _(top: Top[GradualCallable], bottom: Bottom[GradualCallable]) -> None:
    # The top materialization keeps the gradual parameters wrapped
    reveal_type(top)  # revealed: Top[(...) -> object]

    # The bottom materialization simplifies to the fully static bottom callable
    reveal_type(bottom)  # revealed: (*args: object, **kwargs: object) -> Never

# The bottom materialization of a gradual callable is a subtype of (and supertype of)
# a protocol with `__call__(self, *args: object, **kwargs: object) -> Never`
class EquivalentToBottom(Protocol):
    def __call__(self, *args: object, **kwargs: object) -> Never: ...

static_assert(is_subtype_of(EquivalentToBottom, Bottom[Callable[..., Never]]))
static_assert(is_subtype_of(Bottom[Callable[..., Never]], EquivalentToBottom))

# TODO: is_equivalent_to only considers types of the same kind equivalent (Callable vs ProtocolInstance),
# so this fails even though mutual subtyping proves semantic equivalence.
static_assert(is_equivalent_to(Bottom[Callable[..., Never]], EquivalentToBottom))  # error: [static-assert-error]

# Top-materialized callables are not equivalent to non-top-materialized callables, even if their
# signatures would otherwise be equivalent after materialization.
static_assert(not is_equivalent_to(Top[Callable[..., object]], Callable[..., object]))
```

Gradual parameters can be top- and bottom-materialized even if the return type is not `Any`:

```py
type GradualParams = Callable[..., int]

def _(top: Top[GradualParams], bottom: Bottom[GradualParams]) -> None:
    reveal_type(top)  # revealed: Top[(...) -> int]

    reveal_type(bottom)  # revealed: (*args: object, **kwargs: object) -> int
```

Materializing an overloaded callable materializes each overload separately.

```py
from typing import overload
from ty_extensions import CallableTypeOf

@overload
def f(x: int) -> Any: ...
@overload
def f(*args: Any, **kwargs: Any) -> str: ...
def f(*args: object, **kwargs: object) -> object:
    pass

def _(top: Top[CallableTypeOf[f]], bottom: Bottom[CallableTypeOf[f]]):
    reveal_type(top)  # revealed: Overload[(x: int) -> object, Top[(...) -> str]]
    reveal_type(bottom)  # revealed: Overload[(x: int) -> Never, (*args: object, **kwargs: object) -> str]
```

The top callable can be represented in a `ParamSpec`:

```py
def takes_paramspec[**P](f: Callable[P, None]) -> Callable[P, None]:
    return f

def _(top: Top[Callable[..., None]]):
    revealed = takes_paramspec(top)
    reveal_type(revealed)  # revealed: Top[(...) -> None]
```

The top callable is not a subtype of `(*object, **object) -> object`:

```py
type TopCallable = Top[Callable[..., Any]]

@staticmethod
def takes_objects(*args: object, **kwargs: object) -> object:
    pass

static_assert(not is_subtype_of(TopCallable, CallableTypeOf[takes_objects]))
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
    reveal_type(top_ai)  # revealed: Top[list[tuple[Any, int]]]
    reveal_type(bottom_ai)  # revealed: Bottom[list[tuple[Any, int]]]

    reveal_type(top_su)  # revealed: Top[list[tuple[str, Unknown]]]
    reveal_type(bottom_su)  # revealed: Bottom[list[tuple[str, Unknown]]]

    reveal_type(top_aiu)  # revealed: Top[list[tuple[Any, int, Unknown]]]
    reveal_type(bottom_aiu)  # revealed: Bottom[list[tuple[Any, int, Unknown]]]
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
    reveal_type(top_ai)  # revealed: Top[list[Any | int]]
    reveal_type(bottom_ai)  # revealed: Bottom[list[Any | int]]

    reveal_type(top_su)  # revealed: Top[list[str | Unknown]]
    reveal_type(bottom_su)  # revealed: Bottom[list[str | Unknown]]

    reveal_type(top_aiu)  # revealed: Top[list[Any | int]]
    reveal_type(bottom_aiu)  # revealed: Bottom[list[Any | int]]
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
    # Top[list[Any] & list[int]] = Top[list[Any]] & list[int] = list[int]
    reveal_type(top)  # revealed: list[int]
    # Bottom[list[Any] & list[int]] = Bottom[list[Any]] & list[int] = Bottom[list[Any]]
    reveal_type(bottom)  # revealed: Bottom[list[Any]]
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
    reveal_type(top)  # revealed: Top[list[type[Any]]]
    reveal_type(bottom)  # revealed: Bottom[list[type[Any]]]
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
    reveal_type(top)  # revealed: Top[GenericInvariant[Any]]
    reveal_type(bottom)  # revealed: Bottom[GenericInvariant[Any]]

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
    reveal_type(top)  # revealed: (Bottom[GenericInvariant[Any]], /) -> None
    reveal_type(bottom)  # revealed: (Top[GenericInvariant[Any]], /) -> None

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

## Nested use

`Top[T]` and `Bottom[T]` are always fully static types. Therefore, they have only one
materialization (themselves) and applying `Top` or `Bottom` again does nothing.

```py
from typing import Any
from ty_extensions import Top, Bottom, static_assert, is_equivalent_to

static_assert(is_equivalent_to(Top[Top[list[Any]]], Top[list[Any]]))
static_assert(is_equivalent_to(Bottom[Top[list[Any]]], Top[list[Any]]))

static_assert(is_equivalent_to(Bottom[Bottom[list[Any]]], Bottom[list[Any]]))
static_assert(is_equivalent_to(Top[Bottom[list[Any]]], Bottom[list[Any]]))
```

## Subtyping

Any `list[T]` is a subtype of `Top[list[Any]]`, but with more restrictive gradual types, not all
other specializations are subtypes.

```py
from typing import Any, Literal
from ty_extensions import is_subtype_of, static_assert, Top, Intersection, Bottom

# None and Top
static_assert(is_subtype_of(list[int], Top[list[Any]]))
static_assert(not is_subtype_of(Top[list[Any]], list[int]))
static_assert(is_subtype_of(list[bool], Top[list[Intersection[int, Any]]]))
static_assert(is_subtype_of(list[int], Top[list[Intersection[int, Any]]]))
static_assert(not is_subtype_of(list[int | str], Top[list[Intersection[int, Any]]]))
static_assert(not is_subtype_of(list[object], Top[list[Intersection[int, Any]]]))
static_assert(not is_subtype_of(list[str], Top[list[Intersection[int, Any]]]))
static_assert(not is_subtype_of(list[str | bool], Top[list[Intersection[int, Any]]]))

# Top and Top
static_assert(is_subtype_of(Top[list[int | Any]], Top[list[Any]]))
static_assert(not is_subtype_of(Top[list[Any]], Top[list[int | Any]]))
static_assert(is_subtype_of(Top[list[Intersection[int, Any]]], Top[list[Any]]))
static_assert(not is_subtype_of(Top[list[Any]], Top[list[Intersection[int, Any]]]))
static_assert(not is_subtype_of(Top[list[Intersection[int, Any]]], Top[list[int | Any]]))
static_assert(not is_subtype_of(Top[list[int | Any]], Top[list[Intersection[int, Any]]]))
static_assert(not is_subtype_of(Top[list[str | Any]], Top[list[int | Any]]))
static_assert(is_subtype_of(Top[list[str | int | Any]], Top[list[int | Any]]))
static_assert(not is_subtype_of(Top[list[int | Any]], Top[list[str | int | Any]]))

# Bottom and Top
static_assert(is_subtype_of(Bottom[list[Any]], Top[list[Any]]))
static_assert(is_subtype_of(Bottom[list[Any]], Top[list[int | Any]]))
static_assert(is_subtype_of(Bottom[list[int | Any]], Top[list[Any]]))
static_assert(is_subtype_of(Bottom[list[int | Any]], Top[list[int | str]]))
static_assert(is_subtype_of(Bottom[list[Intersection[int, Any]]], Top[list[Intersection[str, Any]]]))
static_assert(not is_subtype_of(Bottom[list[Intersection[int, bool | Any]]], Bottom[list[Intersection[str, Literal["x"] | Any]]]))

# None and None
static_assert(not is_subtype_of(list[int], list[Any]))
static_assert(not is_subtype_of(list[Any], list[int]))
static_assert(is_subtype_of(list[int], list[int]))
static_assert(not is_subtype_of(list[int], list[object]))
static_assert(not is_subtype_of(list[object], list[int]))

# Top and None
static_assert(not is_subtype_of(Top[list[Any]], list[Any]))
static_assert(not is_subtype_of(Top[list[Any]], list[int]))
static_assert(is_subtype_of(Top[list[int]], list[int]))

# Bottom and None
static_assert(is_subtype_of(Bottom[list[Any]], list[object]))
static_assert(is_subtype_of(Bottom[list[int | Any]], list[str | int]))
static_assert(not is_subtype_of(Bottom[list[str | Any]], list[Intersection[int, bool | Any]]))

# None and Bottom
static_assert(not is_subtype_of(list[int], Bottom[list[Any]]))
static_assert(not is_subtype_of(list[int], Bottom[list[int | Any]]))
static_assert(is_subtype_of(list[int], Bottom[list[int]]))

# Top and Bottom
static_assert(not is_subtype_of(Top[list[Any]], Bottom[list[Any]]))
static_assert(not is_subtype_of(Top[list[int | Any]], Bottom[list[int | Any]]))
static_assert(is_subtype_of(Top[list[int]], Bottom[list[int]]))

# Bottom and Bottom
static_assert(is_subtype_of(Bottom[list[Any]], Bottom[list[int | str | Any]]))
static_assert(is_subtype_of(Bottom[list[int | Any]], Bottom[list[int | str | Any]]))
static_assert(is_subtype_of(Bottom[list[bool | Any]], Bottom[list[int | Any]]))
static_assert(not is_subtype_of(Bottom[list[int | Any]], Bottom[list[bool | Any]]))
static_assert(not is_subtype_of(Bottom[list[int | Any]], Bottom[list[Any]]))
```

## Assignability

### General

Assignability is the same as subtyping for top and bottom materializations, because those are fully
static types, but some gradual types are assignable even if they are not subtypes.

```py
from typing import Any, Literal
from ty_extensions import is_assignable_to, static_assert, Top, Intersection, Bottom

# None and Top
static_assert(is_assignable_to(list[Any], Top[list[Any]]))
static_assert(is_assignable_to(list[int], Top[list[Any]]))
static_assert(not is_assignable_to(Top[list[Any]], list[int]))
static_assert(is_assignable_to(list[bool], Top[list[Intersection[int, Any]]]))
static_assert(is_assignable_to(list[int], Top[list[Intersection[int, Any]]]))
static_assert(is_assignable_to(list[Any], Top[list[Intersection[int, Any]]]))
static_assert(not is_assignable_to(list[int | str], Top[list[Intersection[int, Any]]]))
static_assert(not is_assignable_to(list[object], Top[list[Intersection[int, Any]]]))
static_assert(not is_assignable_to(list[str], Top[list[Intersection[int, Any]]]))
static_assert(not is_assignable_to(list[str | bool], Top[list[Intersection[int, Any]]]))

# Top and Top
static_assert(is_assignable_to(Top[list[int | Any]], Top[list[Any]]))
static_assert(not is_assignable_to(Top[list[Any]], Top[list[int | Any]]))
static_assert(is_assignable_to(Top[list[Intersection[int, Any]]], Top[list[Any]]))
static_assert(not is_assignable_to(Top[list[Any]], Top[list[Intersection[int, Any]]]))
static_assert(not is_assignable_to(Top[list[Intersection[int, Any]]], Top[list[int | Any]]))
static_assert(not is_assignable_to(Top[list[int | Any]], Top[list[Intersection[int, Any]]]))
static_assert(not is_assignable_to(Top[list[str | Any]], Top[list[int | Any]]))
static_assert(is_assignable_to(Top[list[str | int | Any]], Top[list[int | Any]]))
static_assert(not is_assignable_to(Top[list[int | Any]], Top[list[str | int | Any]]))

# Bottom and Top
static_assert(is_assignable_to(Bottom[list[Any]], Top[list[Any]]))
static_assert(is_assignable_to(Bottom[list[Any]], Top[list[int | Any]]))
static_assert(is_assignable_to(Bottom[list[int | Any]], Top[list[Any]]))
static_assert(is_assignable_to(Bottom[list[Intersection[int, Any]]], Top[list[Intersection[str, Any]]]))
static_assert(
    not is_assignable_to(Bottom[list[Intersection[int, bool | Any]]], Bottom[list[Intersection[str, Literal["x"] | Any]]])
)

# None and None
static_assert(is_assignable_to(list[int], list[Any]))
static_assert(is_assignable_to(list[Any], list[int]))
static_assert(is_assignable_to(list[int], list[int]))
static_assert(not is_assignable_to(list[int], list[object]))
static_assert(not is_assignable_to(list[object], list[int]))

# Top and None
static_assert(is_assignable_to(Top[list[Any]], list[Any]))
static_assert(not is_assignable_to(Top[list[Any]], list[int]))
static_assert(is_assignable_to(Top[list[int]], list[int]))

# Bottom and None
static_assert(is_assignable_to(Bottom[list[Any]], list[object]))
static_assert(is_assignable_to(Bottom[list[int | Any]], Top[list[str | int]]))
static_assert(not is_assignable_to(Bottom[list[str | Any]], list[Intersection[int, bool | Any]]))

# None and Bottom
static_assert(is_assignable_to(list[Any], Bottom[list[Any]]))
static_assert(not is_assignable_to(list[int], Bottom[list[Any]]))
static_assert(not is_assignable_to(list[int], Bottom[list[int | Any]]))
static_assert(is_assignable_to(list[int], Bottom[list[int]]))

# Top and Bottom
static_assert(not is_assignable_to(Top[list[Any]], Bottom[list[Any]]))
static_assert(not is_assignable_to(Top[list[int | Any]], Bottom[list[int | Any]]))
static_assert(is_assignable_to(Top[list[int]], Bottom[list[int]]))

# Bottom and Bottom
static_assert(is_assignable_to(Bottom[list[Any]], Bottom[list[int | str | Any]]))
static_assert(is_assignable_to(Bottom[list[int | Any]], Bottom[list[int | str | Any]]))
static_assert(is_assignable_to(Bottom[list[bool | Any]], Bottom[list[int | Any]]))
static_assert(not is_assignable_to(Bottom[list[int | Any]], Bottom[list[bool | Any]]))
static_assert(not is_assignable_to(Bottom[list[int | Any]], Bottom[list[Any]]))
```

### Subclasses with different variance

We need to take special care when an invariant class inherits from a covariant or contravariant one.
This comes up frequently in practice because `list` (invariant) inherits from `Sequence` and a
number of other covariant ABCs, but we'll use a synthetic example.

```py
from typing import Generic, TypeVar, Any
from ty_extensions import static_assert, is_assignable_to, is_equivalent_to, Top

class A:
    pass

class B(A):
    pass

T_co = TypeVar("T_co", covariant=True)
T = TypeVar("T")

class CovariantBase(Generic[T_co]):
    def get(self) -> T_co:
        raise NotImplementedError

class InvariantChild(CovariantBase[T]):
    def push(self, obj: T) -> None: ...

static_assert(is_assignable_to(InvariantChild[A], CovariantBase[A]))
static_assert(is_assignable_to(InvariantChild[B], CovariantBase[A]))
static_assert(not is_assignable_to(InvariantChild[A], CovariantBase[B]))
static_assert(not is_assignable_to(InvariantChild[B], InvariantChild[A]))
static_assert(is_equivalent_to(Top[CovariantBase[Any]], CovariantBase[object]))
static_assert(is_assignable_to(InvariantChild[Any], CovariantBase[A]))

static_assert(not is_assignable_to(Top[InvariantChild[Any]], CovariantBase[A]))
```

## Attributes

Attributes on top and bottom materializations are specialized on access.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import Top, Bottom
from typing import Any

class Invariant[T]:
    def get(self) -> T:
        raise NotImplementedError

    def push(self, obj: T) -> None: ...

    attr: T

def capybara(top: Top[Invariant[Any]], bottom: Bottom[Invariant[Any]]) -> None:
    reveal_type(top.get)  # revealed: bound method Top[Invariant[Any]].get() -> object
    reveal_type(top.push)  # revealed: bound method Top[Invariant[Any]].push(obj: Never) -> None

    reveal_type(bottom.get)  # revealed: bound method Bottom[Invariant[Any]].get() -> Never
    reveal_type(bottom.push)  # revealed: bound method Bottom[Invariant[Any]].push(obj: object) -> None

    reveal_type(top.attr)  # revealed: object
    reveal_type(bottom.attr)  # revealed: Never
```
