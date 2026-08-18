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
from ty_extensions import Top
from ty_extensions._internal import Unknown

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
from ty_extensions import Bottom
from ty_extensions._internal import Unknown

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
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import TypeOf, is_equivalent_to
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
from ty_extensions import Bottom, Top
from ty_extensions._internal import Unknown, TypeOf

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
    reveal_type(bottom)  # revealed: (int, tuple[object], /) -> tuple[Never]
```

But, if the callable itself is in a contravariant position, then the variance is flipped i.e., if
the outer variance is covariant, it's flipped to contravariant, and if it's contravariant, it's
flipped to covariant, invariant remains invariant.

```py
type C3 = Callable[[Any, Callable[[Unknown], Any]], Callable[[Any, int], Any]]

def _(top: Top[C3], bottom: Bottom[C3]) -> None:
    # revealed: (Never, (object, /) -> Never, /) -> ((Never, int, /) -> object)
    reveal_type(top)

    # revealed: (object, (Never, /) -> object, /) -> ((object, int, /) -> Never)
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
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to, is_subtype_of

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
static_assert(is_equivalent_to(Bottom[Callable[..., Never]], EquivalentToBottom))

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
from ty_extensions._internal import RegularCallableTypeOf

@overload
def f(x: int) -> Any: ...
@overload
def f(*args: Any, **kwargs: Any) -> str: ...
def f(*args: object, **kwargs: object) -> object:
    pass

def _(top: Top[RegularCallableTypeOf[f]], bottom: Bottom[RegularCallableTypeOf[f]]):
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

static_assert(not is_subtype_of(TopCallable, RegularCallableTypeOf[takes_objects]))
```

## Tuple

All positions in a tuple are covariant.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Never
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import Unknown, is_equivalent_to

static_assert(is_equivalent_to(Top[tuple[Any, int]], tuple[object, int]))
static_assert(is_equivalent_to(Bottom[tuple[Any, int]], tuple[Never, int]))

static_assert(is_equivalent_to(Top[tuple[Unknown, int]], tuple[object, int]))
static_assert(is_equivalent_to(Bottom[tuple[Unknown, int]], tuple[Never, int]))

static_assert(is_equivalent_to(Top[tuple[Any, int, Unknown]], tuple[object, int, object]))
static_assert(is_equivalent_to(Bottom[tuple[Any, int, Unknown]], tuple[Never, int, Never]))
```

Except for when the tuple itself is in a contravariant position, then all positions in the tuple
inherit the contravariant position.

```py
from typing import Callable
from ty_extensions._internal import TypeOf

type C = Callable[[tuple[Any, int], tuple[str, Unknown]], None]

def _(top: Top[C], bottom: Bottom[C]) -> None:
    reveal_type(top)  # revealed: (tuple[Never, int], tuple[str, Never], /) -> None
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
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import Unknown, is_equivalent_to

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
from ty_extensions._internal import TypeOf

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

```pyi
from typing import Any
from typing_extensions import Never
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import Unknown, is_equivalent_to

static_assert(is_equivalent_to(Top[Any & int], int))
static_assert(is_equivalent_to(Bottom[Any & int], Never))

# Here, the top materialization of `Any | int` is `object` and the intersection of it with tuple
static_assert(is_equivalent_to(Top[(Any | int) & tuple[str, Unknown]], tuple[str, object]))
static_assert(is_equivalent_to(Bottom[(Any | int) & tuple[str, Unknown]], Never))

class Foo: ...

static_assert(is_equivalent_to(Bottom[(Any | Foo) & tuple[str]], Foo & tuple[str]))
```

## Intersections of invariant generics

The intersection `list[Any] & list[int]` is eagerly simplified to `list[int]`. Therefore, this is
just a fully-static type where bottom and top materialization are the same:

```pyi
from typing import Any
from ty_extensions import Bottom, Top

def _(
    top: Top[list[Any] & list[int]],
    bottom: Bottom[list[Any] & list[int]],
):
    reveal_type(top)  # revealed: list[int]
    reveal_type(bottom)  # revealed: list[int]
```

Unfortunately, we get a seemingly different result when we distribute `Top[..]` and `Bottom[..]`
over the intersection first:

```pyi
def _(
    top: Top[list[Any]] & Top[list[int]],
    bottom: Bottom[list[Any]] & Bottom[list[int]],
):
    reveal_type(top)  # revealed: list[int]
    reveal_type(bottom)  # revealed: Bottom[list[Any]]
```

This is not a contradiction to what we have above if we view `Bottom[list[Any]]` as an empty
"marker" type that adds no additional materializations. In other words, the gradual type
`Bottom[list[Any]] | list[int] & Any` (i.e. the interval that is spanned by the types of the two
bounds `bottom` and `top`) is equivalent to just `list[int]`.

## Negation

All positions in a negation are contravariant.

```pyi
from typing import Any
from typing_extensions import Never
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import Unknown, is_equivalent_to

# ~Any is still Any, so the top materialization is object
static_assert(is_equivalent_to(Top[~Any], object))
static_assert(is_equivalent_to(Bottom[~Any], Never))

# tuple[Any, int] is in a contravariant position, so its top
# materialization negates the tuple's bottom materialization.
static_assert(is_equivalent_to(Top[~tuple[Any, int]], ~tuple[Never, int]))
static_assert(is_equivalent_to(Bottom[~tuple[Any, int]], ~tuple[object, int]))
```

## `type`

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from typing_extensions import Never
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import Unknown, is_equivalent_to

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

## Materialized class annotations and constructors

A class-object annotation can name either materialization of an invariant generic. Calling the
annotated class produces an instance with the same materialization.

```py
from typing import Any
from ty_extensions import Bottom, Top

def materialized_list_classes(
    top: type[Top[list[Any]]],
    bottom: type[Bottom[list[Any]]],
) -> None:
    reveal_type(top)  # revealed: type[Top[list[Any]]]
    reveal_type(bottom)  # revealed: type[Bottom[list[Any]]]
    reveal_type(top())  # revealed: Top[list[Any]]
    reveal_type(bottom())  # revealed: Bottom[list[Any]]
```

## Generic aliases of materialized classes

A generic class alias can be materialized inside `type[...]`. Aliasing the complete materialized
type also preserves its polarity, and both alias forms resolve to the underlying class.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from ty_extensions import Bottom, Top

type ListAlias[T] = list[T]
type TopList = Top[ListAlias[Any]]
type BottomList = Bottom[ListAlias[Any]]

def aliased_materialized_list_classes(
    generic_top: type[Top[ListAlias[Any]]],
    generic_bottom: type[Bottom[ListAlias[Any]]],
    aliased_top: type[TopList],
    aliased_bottom: type[BottomList],
) -> None:
    reveal_type(generic_top)  # revealed: type[Top[list[Any]]]
    reveal_type(generic_bottom)  # revealed: type[Bottom[list[Any]]]
    reveal_type(aliased_top)  # revealed: type[Top[list[Any]]]
    reveal_type(aliased_bottom)  # revealed: type[Bottom[list[Any]]]
    reveal_type(aliased_top())  # revealed: Top[list[Any]]
    reveal_type(aliased_bottom())  # revealed: Bottom[list[Any]]
```

## Invalid materialization arity in class annotations

`Top` and `Bottom` each require exactly one type argument, even when they are nested inside a
class-object annotation.

```py
from ty_extensions import Bottom, Top

def invalid_materialized_list_classes(
    top: type[Top[int, str]],  # error: [invalid-type-form]
    bottom: type[Bottom[int, str]],  # error: [invalid-type-form]
) -> None: ...
```

## Type variables

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Never, TypeVar
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import Unknown, is_subtype_of

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
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

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

When all invariant type parameters are fully static (e.g. type variables rather than gradual types
like `Any`), `Top` simplifies away since there is no dynamic component to materialize:

```py
class Foo: ...

T_bounded = TypeVar("T_bounded", bound=Foo)
T_unbounded = TypeVar("T_unbounded")

class InvariantBounded(Generic[T_bounded]):
    x: T_bounded

class InvariantUnbounded(Generic[T_unbounded]):
    x: T_unbounded

def f(
    bounded: Top[InvariantBounded[T_bounded]],
    unbounded: Top[InvariantUnbounded[T_unbounded]],
):
    reveal_type(bounded)  # revealed: InvariantBounded[T_bounded@f]
    reveal_type(unbounded)  # revealed: InvariantUnbounded[T_unbounded@f]
```

Parameters in callable are contravariant, so the variance should be flipped:

```py
from typing import Callable
from ty_extensions._internal import TypeOf

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

## Bounded generic type parameters

Top materialization of a covariant generic uses the type parameter's declared upper bound. Bottom
materialization uses its lower bound, `Never`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Generic, Never, TypeVar
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to, is_subtype_of

class BoundedCovariant[T: int]:
    def get(self) -> T:
        raise NotImplementedError

static_assert(is_equivalent_to(Top[BoundedCovariant[Any]], BoundedCovariant[int]))
static_assert(is_equivalent_to(Bottom[BoundedCovariant[Any]], BoundedCovariant[Never]))
static_assert(is_subtype_of(BoundedCovariant[Any], Top[BoundedCovariant[Any]]))
static_assert(is_subtype_of(BoundedCovariant[Any], BoundedCovariant[int]))
```

A type alias can conceal a gradual argument; the same subtype relationships still apply.

```py
type AliasedAny = Any

static_assert(is_subtype_of(BoundedCovariant[AliasedAny], Top[BoundedCovariant[AliasedAny]]))
static_assert(is_subtype_of(BoundedCovariant[AliasedAny], BoundedCovariant[int]))
```

An alias for a static upper bound remains static. It absorbs a bounded gradual specialization in
either union order.

```py
type AliasedInt = int

def aliased_static_bound(
    gradual_first: BoundedCovariant[Any] | BoundedCovariant[AliasedInt],
    gradual_last: BoundedCovariant[AliasedInt] | BoundedCovariant[Any],
) -> None:
    reveal_type(gradual_first)  # revealed: BoundedCovariant[AliasedInt]
    reveal_type(gradual_last)  # revealed: BoundedCovariant[AliasedInt]
```

Contravariance reverses which bound is used by top and bottom materialization.

```py
class BoundedContravariant[T: int]:
    def put(self, value: T) -> None: ...

static_assert(is_equivalent_to(Top[BoundedContravariant[Any]], BoundedContravariant[Never]))
static_assert(is_equivalent_to(Bottom[BoundedContravariant[Any]], BoundedContravariant[int]))
```

For an invariant generic, materialize attributes and method parameters according to their own
variance. An unrelated `Any` attribute must remain gradual.

```py
class BoundedInvariant[T: int]:
    value: T
    unrelated: Any

    def get(self) -> T:
        raise NotImplementedError

    def put(self, value: T) -> None: ...

def bounded_invariant(
    top: Top[BoundedInvariant[Any]],
    bottom: Bottom[BoundedInvariant[Any]],
) -> None:
    reveal_type(top.value)  # revealed: int
    reveal_type(top.unrelated)  # revealed: Any
    reveal_type(top.get)  # revealed: bound method Top[BoundedInvariant[Any]].get() -> int
    reveal_type(top.put)  # revealed: bound method Top[BoundedInvariant[Any]].put(value: Never) -> None

    reveal_type(bottom.unrelated)  # revealed: Any
    reveal_type(bottom.get)  # revealed: bound method Bottom[BoundedInvariant[Any]].get() -> Never
    reveal_type(bottom.put)  # revealed: bound method Bottom[BoundedInvariant[Any]].put(value: int) -> None
    reveal_type(bottom.value)  # revealed: Never
```

Explicitly covariant and contravariant legacy `TypeVar` declarations obey the same bounded
materialization rules.

```py
BoundedT_co = TypeVar("BoundedT_co", bound=int, covariant=True)

class LegacyBoundedCovariant(Generic[BoundedT_co]): ...

static_assert(is_equivalent_to(Top[LegacyBoundedCovariant[Any]], LegacyBoundedCovariant[int]))
static_assert(is_equivalent_to(Bottom[LegacyBoundedCovariant[Any]], LegacyBoundedCovariant[Never]))

BoundedT_contra = TypeVar("BoundedT_contra", bound=int, contravariant=True)

class LegacyBoundedContravariant(Generic[BoundedT_contra]): ...

static_assert(is_equivalent_to(Top[LegacyBoundedContravariant[Any]], LegacyBoundedContravariant[Never]))
static_assert(is_equivalent_to(Bottom[LegacyBoundedContravariant[Any]], LegacyBoundedContravariant[int]))
```

Reading an attribute of a top-materialized legacy invariant generic yields the type parameter's
upper bound; reading the same attribute from its bottom materialization yields the lower bound.

```py
BoundedT = TypeVar("BoundedT", bound=int)

class LegacyBoundedInvariant(Generic[BoundedT]):
    value: BoundedT

def legacy_bounded_invariant(
    legacy_top: Top[LegacyBoundedInvariant[Any]],
    legacy_bottom: Bottom[LegacyBoundedInvariant[Any]],
) -> None:
    reveal_type(legacy_top.value)  # revealed: int
    reveal_type(legacy_bottom.value)  # revealed: Never
```

## Constrained generic type parameters

A constrained type parameter cannot generally be replaced by the union of its constraints: the union
need not itself be a valid specialization. Top and bottom materialization must instead retain the
covariant generic and its valid specializations.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, Generic, Never, TypeVar
from ty_extensions import Bottom, Intersection, Not, Top, static_assert
from ty_extensions._internal import is_assignable_to, is_equivalent_to, is_subtype_of

class ConstrainedCovariant[T: (int, str)]:
    def get(self) -> T:
        raise NotImplementedError

def constrained_covariant(
    top: Top[ConstrainedCovariant[Any]],
    bottom: Bottom[ConstrainedCovariant[Any]],
) -> None:
    reveal_type(top)  # revealed: Top[ConstrainedCovariant[Any]]
    reveal_type(bottom)  # revealed: Bottom[ConstrainedCovariant[Any]]

static_assert(is_subtype_of(ConstrainedCovariant[int], Top[ConstrainedCovariant[Any]]))
static_assert(is_subtype_of(ConstrainedCovariant[str], Top[ConstrainedCovariant[Any]]))
static_assert(is_subtype_of(ConstrainedCovariant[Any], Top[ConstrainedCovariant[Any]]))
static_assert(not is_subtype_of(Top[ConstrainedCovariant[Any]], ConstrainedCovariant[int]))
static_assert(not is_subtype_of(Top[ConstrainedCovariant[Any]], ConstrainedCovariant[str]))
static_assert(is_subtype_of(Bottom[ConstrainedCovariant[Any]], ConstrainedCovariant[int]))
static_assert(is_subtype_of(Bottom[ConstrainedCovariant[Any]], ConstrainedCovariant[str]))
static_assert(is_subtype_of(Bottom[ConstrainedCovariant[Any]], Top[ConstrainedCovariant[Any]]))
static_assert(not is_equivalent_to(Intersection[ConstrainedCovariant[str], Not[ConstrainedCovariant[int]]], Never))

static_assert(is_assignable_to(ConstrainedCovariant[int], Top[ConstrainedCovariant[Any]]))
static_assert(not is_assignable_to(Top[ConstrainedCovariant[Any]], ConstrainedCovariant[int]))
static_assert(is_assignable_to(Bottom[ConstrainedCovariant[Any]], ConstrainedCovariant[int]))
```

Contravariant constrained generics likewise preserve their materializations while reversing the
relationship between input positions and top or bottom types.

```py
class ConstrainedContravariant[T: (int, str)]:
    def put(self, value: T) -> None: ...

def constrained_contravariant(
    top: Top[ConstrainedContravariant[Any]],
    bottom: Bottom[ConstrainedContravariant[Any]],
) -> None:
    reveal_type(top)  # revealed: Top[ConstrainedContravariant[Any]]
    reveal_type(bottom)  # revealed: Bottom[ConstrainedContravariant[Any]]

static_assert(is_subtype_of(ConstrainedContravariant[int], Top[ConstrainedContravariant[Any]]))
static_assert(is_subtype_of(ConstrainedContravariant[str], Top[ConstrainedContravariant[Any]]))
static_assert(not is_subtype_of(Top[ConstrainedContravariant[Any]], ConstrainedContravariant[int]))
static_assert(not is_subtype_of(Top[ConstrainedContravariant[Any]], ConstrainedContravariant[str]))
static_assert(is_subtype_of(Bottom[ConstrainedContravariant[Any]], ConstrainedContravariant[int]))
static_assert(is_subtype_of(Bottom[ConstrainedContravariant[Any]], ConstrainedContravariant[str]))
static_assert(is_subtype_of(Bottom[ConstrainedContravariant[Any]], Top[ConstrainedContravariant[Any]]))

static_assert(is_assignable_to(ConstrainedContravariant[int], Top[ConstrainedContravariant[Any]]))
static_assert(not is_assignable_to(Top[ConstrainedContravariant[Any]], ConstrainedContravariant[int]))
static_assert(is_assignable_to(Bottom[ConstrainedContravariant[Any]], ConstrainedContravariant[int]))
```

An invariant constrained parameter materializes readable values to the union of valid constraints
and writable parameters to `Never`. Unrelated gradual attributes remain `Any`.

```py
class ConstrainedInvariant[T: (int, str)]:
    value: T
    unrelated: Any

    def get(self) -> T:
        raise NotImplementedError

    def put(self, value: T) -> None: ...

def constrained_invariant(
    top: Top[ConstrainedInvariant[Any]],
    bottom: Bottom[ConstrainedInvariant[Any]],
) -> None:
    reveal_type(top.value)  # revealed: int | str
    reveal_type(top.unrelated)  # revealed: Any
    reveal_type(top.get)  # revealed: bound method Top[ConstrainedInvariant[Any]].get() -> int | str
    reveal_type(top.put)  # revealed: bound method Top[ConstrainedInvariant[Any]].put(value: Never) -> None

    reveal_type(bottom.unrelated)  # revealed: Any
    reveal_type(bottom.get)  # revealed: bound method Bottom[ConstrainedInvariant[Any]].get() -> Never
    reveal_type(bottom.put)  # revealed: bound method Bottom[ConstrainedInvariant[Any]].put(value: int | str) -> None
    reveal_type(bottom.value)  # revealed: Never
```

Direct attribute writes are currently checked against the readable union rather than the safe
`Never` parameter used for setters.

```py
def constrained_invariant_writes(top: Top[ConstrainedInvariant[Any]]) -> None:
    # TODO: Reject these writes; neither value is safe for every specialization.
    top.value = 1
    top.value = "value"
    top.value = 1.5  # error: [invalid-assignment]
```

Legacy constrained type variables preserve the same covariant and contravariant subtype
relationships.

```py
ConstrainedT_co = TypeVar("ConstrainedT_co", int, str, covariant=True)

class LegacyConstrainedCovariant(Generic[ConstrainedT_co]): ...

static_assert(is_subtype_of(LegacyConstrainedCovariant[int], Top[LegacyConstrainedCovariant[Any]]))
static_assert(is_subtype_of(Bottom[LegacyConstrainedCovariant[Any]], LegacyConstrainedCovariant[str]))

ConstrainedT_contra = TypeVar("ConstrainedT_contra", int, str, contravariant=True)

class LegacyConstrainedContravariant(Generic[ConstrainedT_contra]): ...

static_assert(is_subtype_of(LegacyConstrainedContravariant[int], Top[LegacyConstrainedContravariant[Any]]))
static_assert(is_subtype_of(Bottom[LegacyConstrainedContravariant[Any]], LegacyConstrainedContravariant[str]))
```

A partially gradual type argument filters out constraints incompatible with its static `int` arm.

```py
static_assert(is_equivalent_to(Bottom[ConstrainedCovariant[Any | int]], ConstrainedCovariant[int]))
static_assert(not is_subtype_of(ConstrainedCovariant[str], Top[ConstrainedCovariant[Any | int]]))
static_assert(is_equivalent_to(Top[ConstrainedContravariant[Any | int]], ConstrainedContravariant[int]))
static_assert(not is_subtype_of(Bottom[ConstrainedContravariant[Any | int]], ConstrainedContravariant[str]))
```

An intersection of `int` and `Any` likewise retains only the compatible `int` constraint for both
variances.

```py
type GradualInt = Intersection[int, Any]

static_assert(is_subtype_of(ConstrainedCovariant[int], Top[ConstrainedCovariant[GradualInt]]))
static_assert(not is_subtype_of(ConstrainedCovariant[str], Top[ConstrainedCovariant[GradualInt]]))
static_assert(is_subtype_of(Bottom[ConstrainedCovariant[GradualInt]], ConstrainedCovariant[int]))
static_assert(not is_subtype_of(Bottom[ConstrainedCovariant[GradualInt]], ConstrainedCovariant[str]))
static_assert(is_subtype_of(ConstrainedContravariant[int], Top[ConstrainedContravariant[GradualInt]]))
static_assert(not is_subtype_of(ConstrainedContravariant[str], Top[ConstrainedContravariant[GradualInt]]))
static_assert(is_subtype_of(Bottom[ConstrainedContravariant[GradualInt]], ConstrainedContravariant[int]))
static_assert(not is_subtype_of(Bottom[ConstrainedContravariant[GradualInt]], ConstrainedContravariant[str]))
```

## Gradual generic constraints

When `Any` is itself a constraint, static specializations outside the other constraint must remain
valid. Reading a top-materialized covariant value produces `object`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_subtype_of

class GradualConstrainedCovariant[T: (int, Any)]:
    def get(self) -> T:
        raise NotImplementedError

class GradualConstrainedContravariant[T: (int, Any)]:
    def put(self, value: T) -> None: ...

def gradual_constraints(value: Top[GradualConstrainedCovariant[Any]]) -> None:
    reveal_type(value)  # revealed: Top[GradualConstrainedCovariant[Any]]
    reveal_type(value.get())  # revealed: object

static_assert(is_subtype_of(GradualConstrainedCovariant[int], Top[GradualConstrainedCovariant[Any]]))
static_assert(is_subtype_of(GradualConstrainedCovariant[str], Top[GradualConstrainedCovariant[Any]]))
static_assert(is_subtype_of(Bottom[GradualConstrainedCovariant[Any]], GradualConstrainedCovariant[int]))
static_assert(is_subtype_of(Bottom[GradualConstrainedCovariant[Any]], GradualConstrainedCovariant[str]))
static_assert(is_subtype_of(GradualConstrainedContravariant[int], Top[GradualConstrainedContravariant[Any]]))
static_assert(is_subtype_of(GradualConstrainedContravariant[str], Top[GradualConstrainedContravariant[Any]]))
static_assert(is_subtype_of(Bottom[GradualConstrainedContravariant[Any]], GradualConstrainedContravariant[int]))
static_assert(is_subtype_of(Bottom[GradualConstrainedContravariant[Any]], GradualConstrainedContravariant[str]))
```

## Overlapping generic constraints

When one valid constraint is a subtype of another, the broader constraint supplies the upper bound
and the narrower constraint supplies the lower bound.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

class OverlappingCovariant[T: (int, bool)]:
    def get(self) -> T:
        raise NotImplementedError

class OverlappingContravariant[T: (int, bool)]:
    def put(self, value: T) -> None: ...

static_assert(is_equivalent_to(Top[OverlappingCovariant[Any]], OverlappingCovariant[int]))
static_assert(is_equivalent_to(Bottom[OverlappingCovariant[Any]], OverlappingCovariant[bool]))
static_assert(is_equivalent_to(Top[OverlappingContravariant[Any]], OverlappingContravariant[bool]))
static_assert(is_equivalent_to(Bottom[OverlappingContravariant[Any]], OverlappingContravariant[int]))
```

## Mixed constrained and unconstrained type parameters

A generic with both constrained and unconstrained parameters materializes each parameter
independently. Filtering the constrained parameter must not change the unconstrained parameter.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from ty_extensions import Bottom, Intersection, Top, static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

type GradualInt = Intersection[int, Any]

class MixedConstrained[T: (int, str), U]:
    value: T
    items: list[U]

def mixed_constrained(
    top: Top[MixedConstrained[Any, Any]],
    bottom: Bottom[MixedConstrained[Any, Any]],
) -> None:
    reveal_type(top)  # revealed: Top[MixedConstrained[Any, Any]]
    reveal_type(bottom)  # revealed: Bottom[MixedConstrained[Any, Any]]
    reveal_type(top.value)  # revealed: int | str
    reveal_type(top.items)  # revealed: Top[list[Any]]
    reveal_type(bottom.items)  # revealed: Bottom[list[Any]]
    reveal_type(bottom.value)  # revealed: Never

static_assert(is_subtype_of(MixedConstrained[int, int], Top[MixedConstrained[Any, Any]]))
static_assert(is_subtype_of(MixedConstrained[str, int], Top[MixedConstrained[Any, Any]]))
static_assert(is_subtype_of(Bottom[MixedConstrained[Any, Any]], MixedConstrained[int, int]))
static_assert(is_subtype_of(Bottom[MixedConstrained[Any, Any]], MixedConstrained[str, int]))
static_assert(is_subtype_of(MixedConstrained[int, int], Top[MixedConstrained[Any, int]]))
static_assert(is_assignable_to(MixedConstrained[int, str], Top[MixedConstrained[GradualInt, Any]]))
static_assert(not is_assignable_to(MixedConstrained[str, str], Top[MixedConstrained[GradualInt, Any]]))
static_assert(is_assignable_to(Bottom[MixedConstrained[GradualInt, Any]], MixedConstrained[int, int]))
static_assert(not is_assignable_to(Bottom[MixedConstrained[GradualInt, Any]], MixedConstrained[str, int]))
```

## Materialization does not force invalid recursive specializations

An invalid self-referential bound must produce the expected diagnostics without forcing recursive
materialization. Invalid specializations recover as `Unknown`.

```toml
[environment]
python-version = "3.12"
```

```py
# error: [invalid-type-arguments]
class RecursiveSpecialization[T: "RecursiveSpecialization[int]"]: ...

# error: [invalid-type-arguments]
def recursive_specialization(value: RecursiveSpecialization[str]) -> None:
    reveal_type(value)  # revealed: RecursiveSpecialization[Unknown]
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

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from ty_extensions import Top, Bottom, static_assert
from ty_extensions._internal import is_equivalent_to

static_assert(is_equivalent_to(Top[Top[list[Any]]], Top[list[Any]]))
static_assert(is_equivalent_to(Bottom[Top[list[Any]]], Top[list[Any]]))

static_assert(is_equivalent_to(Bottom[Bottom[list[Any]]], Bottom[list[Any]]))
static_assert(is_equivalent_to(Top[Bottom[list[Any]]], Bottom[list[Any]]))
```

The same is true when a covariant specialization contains a recursive alias with a gradual invariant
branch. Materializing the recursive branch again must not unfold another layer.

```py
class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

class Invariant[T]:
    value: T

type Recursive = Covariant[Recursive] | Invariant[Any]

static_assert(is_equivalent_to(Top[Covariant[Recursive]], Top[Top[Covariant[Recursive]]]))
static_assert(is_equivalent_to(Bottom[Covariant[Recursive]], Bottom[Bottom[Covariant[Recursive]]]))
static_assert(is_equivalent_to(Top[Covariant[Recursive]], Bottom[Top[Covariant[Recursive]]]))
static_assert(is_equivalent_to(Bottom[Covariant[Recursive]], Top[Bottom[Covariant[Recursive]]]))
```

Both branches retain the requested materialization polarity.

```py
def recursive_materializations(top: Top[Recursive], bottom: Bottom[Recursive]) -> None:
    reveal_type(top)  # revealed: Covariant[Top[Recursive]] | Top[Invariant[Any]]
    reveal_type(bottom)  # revealed: Covariant[Bottom[Recursive]] | Bottom[Invariant[Any]]
```

Nested recursive aliases preserve their materialization polarity in displays and diagnostics.

```py
def nested_recursive_materializations(top: Top[Covariant[Recursive]], bottom: Bottom[Covariant[Recursive]]) -> None:
    reveal_type(top)  # revealed: Covariant[Top[Recursive]]
    reveal_type(bottom)  # revealed: Covariant[Bottom[Recursive]]

    # error: [invalid-assignment] "Object of type `Covariant[Top[Recursive]]` is not assignable to `Covariant[Bottom[Recursive]]`"
    bottom = top
```

Explicitly constructed recursive aliases preserve the same materialized identity.

```py
from typing_extensions import TypeAliasType

ManualRecursive = TypeAliasType("ManualRecursive", "Covariant[ManualRecursive] | Invariant[Any]")

static_assert(is_equivalent_to(Top[Covariant[ManualRecursive]], Top[Top[Covariant[ManualRecursive]]]))
```

Materialization also preserves the specialization of a recursive generic alias.

```py
type GenericRecursive[T] = Covariant[GenericRecursive[T]] | Invariant[Any] | T

static_assert(is_equivalent_to(Top[GenericRecursive[int]], Top[Top[GenericRecursive[int]]]))
static_assert(not is_equivalent_to(Top[GenericRecursive[int]], Top[GenericRecursive[str]]))

def generic_recursive_materialization(value: Top[Covariant[GenericRecursive[int]]]) -> None:
    reveal_type(value)  # revealed: Covariant[Top[GenericRecursive[int]]]
```

## Subtyping

Any `list[T]` is a subtype of `Top[list[Any]]`, but with more restrictive gradual types, not all
other specializations are subtypes.

```pyi
from typing import Any, Literal
from ty_extensions import static_assert, Top, Bottom
from ty_extensions._internal import is_subtype_of

# None and Top
static_assert(is_subtype_of(list[int], Top[list[Any]]))
static_assert(not is_subtype_of(Top[list[Any]], list[int]))
static_assert(is_subtype_of(list[bool], Top[list[int & Any]]))
static_assert(is_subtype_of(list[int], Top[list[int & Any]]))
static_assert(not is_subtype_of(list[int | str], Top[list[int & Any]]))
static_assert(not is_subtype_of(list[object], Top[list[int & Any]]))
static_assert(not is_subtype_of(list[str], Top[list[int & Any]]))
static_assert(not is_subtype_of(list[str | bool], Top[list[int & Any]]))

# Top and Top
static_assert(is_subtype_of(Top[list[int | Any]], Top[list[Any]]))
static_assert(not is_subtype_of(Top[list[Any]], Top[list[int | Any]]))
static_assert(is_subtype_of(Top[list[int & Any]], Top[list[Any]]))
static_assert(not is_subtype_of(Top[list[Any]], Top[list[int & Any]]))
static_assert(not is_subtype_of(Top[list[int & Any]], Top[list[int | Any]]))
static_assert(not is_subtype_of(Top[list[int | Any]], Top[list[int & Any]]))
static_assert(not is_subtype_of(Top[list[str | Any]], Top[list[int | Any]]))
static_assert(is_subtype_of(Top[list[str | int | Any]], Top[list[int | Any]]))
static_assert(not is_subtype_of(Top[list[int | Any]], Top[list[str | int | Any]]))

# Bottom and Top
static_assert(is_subtype_of(Bottom[list[Any]], Top[list[Any]]))
static_assert(is_subtype_of(Bottom[list[Any]], Top[list[int | Any]]))
static_assert(is_subtype_of(Bottom[list[int | Any]], Top[list[Any]]))
static_assert(is_subtype_of(Bottom[list[int | Any]], Top[list[int | str]]))
static_assert(is_subtype_of(Bottom[list[int & Any]], Top[list[str & Any]]))
static_assert(not is_subtype_of(Bottom[list[int & (bool | Any)]], Bottom[list[str & (Literal["x"] | Any)]]))

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
static_assert(not is_subtype_of(Bottom[list[str | Any]], list[int & (bool | Any)]))

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

```pyi
from typing import Any, Literal
from ty_extensions import static_assert, Top, Bottom
from ty_extensions._internal import is_assignable_to

# None and Top
static_assert(is_assignable_to(list[Any], Top[list[Any]]))
static_assert(is_assignable_to(list[int], Top[list[Any]]))
static_assert(not is_assignable_to(Top[list[Any]], list[int]))
static_assert(is_assignable_to(list[bool], Top[list[int & Any]]))
static_assert(is_assignable_to(list[int], Top[list[int & Any]]))
static_assert(is_assignable_to(list[Any], Top[list[int & Any]]))
static_assert(not is_assignable_to(list[int | str], Top[list[int & Any]]))
static_assert(not is_assignable_to(list[object], Top[list[int & Any]]))
static_assert(not is_assignable_to(list[str], Top[list[int & Any]]))
static_assert(not is_assignable_to(list[str | bool], Top[list[int & Any]]))

# Top and Top
static_assert(is_assignable_to(Top[list[int | Any]], Top[list[Any]]))
static_assert(not is_assignable_to(Top[list[Any]], Top[list[int | Any]]))
static_assert(is_assignable_to(Top[list[int & Any]], Top[list[Any]]))
static_assert(not is_assignable_to(Top[list[Any]], Top[list[int & Any]]))
static_assert(not is_assignable_to(Top[list[int & Any]], Top[list[int | Any]]))
static_assert(not is_assignable_to(Top[list[int | Any]], Top[list[int & Any]]))
static_assert(not is_assignable_to(Top[list[str | Any]], Top[list[int | Any]]))
static_assert(is_assignable_to(Top[list[str | int | Any]], Top[list[int | Any]]))
static_assert(not is_assignable_to(Top[list[int | Any]], Top[list[str | int | Any]]))

# Bottom and Top
static_assert(is_assignable_to(Bottom[list[Any]], Top[list[Any]]))
static_assert(is_assignable_to(Bottom[list[Any]], Top[list[int | Any]]))
static_assert(is_assignable_to(Bottom[list[int | Any]], Top[list[Any]]))
static_assert(is_assignable_to(Bottom[list[int & Any]], Top[list[str & Any]]))
static_assert(not is_assignable_to(Bottom[list[int & (bool | Any)]], Bottom[list[str & (Literal["x"] | Any)]]))

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
static_assert(not is_assignable_to(Bottom[list[str | Any]], list[int & (bool | Any)]))

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
from ty_extensions import static_assert, Top
from ty_extensions._internal import is_assignable_to, is_equivalent_to

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

def slice_list(top: Top[list[Any]], bottom: Bottom[list[Any]]) -> None:
    reveal_type(top[:])  # revealed: Top[list[Any]]
    reveal_type(bottom[:])  # revealed: Bottom[list[Any]]

class Mixed[T, U]:
    first: T
    second: U
    nested: list[tuple[Any, U]]

def preserve_unrelated_any(top: Top[Mixed[Any, int]], bottom: Bottom[Mixed[Any, int]]) -> None:
    reveal_type(top.nested)  # revealed: list[tuple[Any, int]]
    reveal_type(bottom.nested)  # revealed: list[tuple[Any, int]]
```

Alias specializations also preserve the materialization polarity in contravariant positions.

```py
from ty_extensions import Top, Bottom
from typing import Any, Callable

type Alias[T] = T
type AliasedCallable[T] = Callable[[Alias[T]], T]

def _(top: Top[AliasedCallable[Any]], bottom: Bottom[AliasedCallable[Any]]) -> None:
    reveal_type(top)  # revealed: (Never, /) -> object
    reveal_type(bottom)  # revealed: (object, /) -> Never
```

When a materialized class specialization is applied to an attribute, the same function-literal type
can be visited through both the parameter and return positions of a nested callable. Those positions
use opposite materialization polarities and must not share a transformation cache.

```py
from ty_extensions import Top, Bottom
from typing import Any, Callable
from ty_extensions._internal import TypeOf

class FunctionHolder[T]:
    def shared(self, value: T) -> T:
        raise NotImplementedError

    nested: Callable[[TypeOf[shared]], TypeOf[shared]]

def _(top: Top[FunctionHolder[Any]], bottom: Bottom[FunctionHolder[Any]]) -> None:
    # revealed: (def shared(self, value: object) -> Never, /) -> def shared(self, value: Never) -> object
    reveal_type(top.nested)

    # revealed: (def shared(self, value: Never) -> object, /) -> def shared(self, value: object) -> Never
    reveal_type(bottom.nested)
```

## Protocols

Materializing a protocol maps each member according to how it is used. Reads are covariant and
writes are contravariant.

```toml
[environment]
python-version = "3.12"
```

### Instance attributes

For a mutable `Any` attribute, `Top` reads `object` and writes `Never`; `Bottom` does the reverse:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class MutableAny(Protocol):
    value: Any

def mutable_top_attributes(top: Top[MutableAny]) -> None:
    reveal_type(top)  # revealed: Top[MutableAny]
    reveal_type(top.value)  # revealed: object
    top.value = 1  # error: [invalid-assignment]

def mutable_bottom_attributes(bottom: Bottom[MutableAny]) -> None:
    reveal_type(bottom)  # revealed: Bottom[MutableAny]
    bottom.value = object()
    reveal_type(bottom.value)  # revealed: Never
```

The class object of a materialized protocol preserves its instance type when called directly or
passed through a generic callable:

```py
from typing import Callable

def invoke[T](factory: Callable[[], T]) -> T:
    return factory()

def constructors(top: Top[MutableAny]) -> None:
    reveal_type(type(top))  # revealed: type[Top[MutableAny]]
    reveal_type(type(top)())  # revealed: Top[MutableAny]
    reveal_type(invoke(type(top)))  # revealed: Top[MutableAny]

def annotated_constructors(top: type[Top[MutableAny]], bottom: type[Bottom[MutableAny]]) -> None:
    reveal_type(top)  # revealed: type[Top[MutableAny]]
    reveal_type(bottom)  # revealed: type[Bottom[MutableAny]]
    reveal_type(top())  # revealed: Top[MutableAny]
    reveal_type(bottom())  # revealed: Bottom[MutableAny]
    reveal_type(invoke(top))  # revealed: Top[MutableAny]
    reveal_type(invoke(bottom))  # revealed: Bottom[MutableAny]
```

A protocol's constructor can explicitly return a value that is not an instance of the protocol.
Materializing the protocol must preserve that return type, including when its class object is
converted to a callable:

```py
class IntConstructor(Protocol):
    value: Any

    def __new__(cls) -> int:
        return 1

def non_instance_constructors(
    plain: type[IntConstructor],
    top: type[Top[IntConstructor]],
    bottom: type[Bottom[IntConstructor]],
) -> None:
    reveal_type(invoke(plain))  # revealed: int
    reveal_type(invoke(top))  # revealed: int
    reveal_type(invoke(bottom))  # revealed: int
```

A custom protocol metaclass can likewise construct a value that is not a protocol instance. Its
`__call__` return type is preserved when the protocol is materialized.

```py
class IntConstructorMetaclass(type(Protocol)):
    def __call__(cls) -> int:
        return 1

class MetaclassConstructor(Protocol, metaclass=IntConstructorMetaclass):
    value: Any

def metaclass_constructors(
    plain: type[MetaclassConstructor],
    top: type[Top[MetaclassConstructor]],
    bottom: type[Bottom[MetaclassConstructor]],
) -> None:
    reveal_type(invoke(plain))  # revealed: int
    reveal_type(invoke(top))  # revealed: int
    reveal_type(invoke(bottom))  # revealed: int
```

Overloaded constructors preserve each return type separately: an instance-returning overload uses
the materialized protocol, while an overload returning a different type retains that type.

```py
from typing import Self, overload

class MixedConstructor(Protocol):
    value: Any

    @overload
    def __new__(cls) -> Self: ...
    @overload
    def __new__(cls, value: int) -> int: ...
    def __new__(cls, value: int | None = None) -> Self | int:
        raise NotImplementedError

def invoke_with_int[T](factory: Callable[[int], T]) -> T:
    return factory(1)

def mixed_constructors(
    top: type[Top[MixedConstructor]],
    bottom: type[Bottom[MixedConstructor]],
) -> None:
    reveal_type(invoke(top))  # revealed: Top[MixedConstructor]
    reveal_type(invoke(bottom))  # revealed: Bottom[MixedConstructor]
    reveal_type(invoke_with_int(top))  # revealed: int
    reveal_type(invoke_with_int(bottom))  # revealed: int
```

Materialization preserves sound class-member access: an ordinary instance attribute is not available
on `type[Top[MutableAny]]` or `type[Bottom[MutableAny]]`.

```py
def class_instance_attributes(top: Top[MutableAny], bottom: Bottom[MutableAny]) -> None:
    type(top).value  # error: [unresolved-attribute]
    type(bottom).value  # error: [unresolved-attribute]

def annotated_class_instance_attributes(top: type[Top[MutableAny]], bottom: type[Bottom[MutableAny]]) -> None:
    top.value  # error: [unresolved-attribute]
    bottom.value  # error: [unresolved-attribute]
```

### Writable properties

A property setter is already a write, so its parameter is mapped only once:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class WritableAny(Protocol):
    @property
    def value(self) -> Any: ...
    @value.setter
    def value(self, value: Any) -> None: ...

def writable_top_property(top: Top[WritableAny]) -> None:
    reveal_type(top.value)  # revealed: object
    top.value = 1  # error: [invalid-assignment]

def writable_bottom_property(bottom: Bottom[WritableAny]) -> None:
    bottom.value = object()
    reveal_type(bottom.value)  # revealed: Never
```

### Protocol relations

`MutableAny` and `Top[MutableAny]` refer to the same protocol class, but they do not have the same
read and write requirements. Subtyping and union simplification must use those requirements:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_subtype_of

class MutableAny(Protocol):
    value: Any

static_assert(is_subtype_of(Bottom[MutableAny], MutableAny))
static_assert(is_subtype_of(Bottom[MutableAny], Top[MutableAny]))
static_assert(is_subtype_of(MutableAny, Top[MutableAny]))
static_assert(not is_subtype_of(MutableAny, Bottom[MutableAny]))
static_assert(not is_subtype_of(Top[MutableAny], Bottom[MutableAny]))
static_assert(not is_subtype_of(Top[MutableAny], MutableAny))

def union_order(
    plain_first: MutableAny | Top[MutableAny],
    top_first: Top[MutableAny] | MutableAny,
) -> None:
    reveal_type(plain_first)  # revealed: Top[MutableAny]
    reveal_type(top_first)  # revealed: Top[MutableAny]
    reveal_type(plain_first.value)  # revealed: object
    reveal_type(top_first.value)  # revealed: object
```

Inheriting from a protocol must not bypass its materialized write requirement. A nominal subclass
and a structurally identical class therefore have the same result here:

```py
class MutableAnySubclass(MutableAny):
    value: int

class StructuralMutableAny:
    value: int

static_assert(not is_subtype_of(MutableAnySubclass, Bottom[MutableAny]))
static_assert(not is_subtype_of(StructuralMutableAny, Bottom[MutableAny]))
```

An inherited `Any` member is materialized along with members declared directly on the protocol, so
it cannot satisfy a more specific inherited protocol:

```py
class GenericBase[T](Protocol):
    item: T

class InheritedAny(GenericBase[Any], Protocol):
    marker: Any

def requires_int_base(value: GenericBase[int]) -> None: ...
def _(top: Top[InheritedAny]) -> None:
    requires_int_base(top)  # error: [invalid-argument-type]
```

Materializing an unrelated member does not erase explicit protocol inheritance, even when an
override is structurally incompatible with the base protocol. Materializing the fully static base
also preserves the nominal relationship:

```py
class BaseProtocol(Protocol):
    @property
    def value(self) -> int: ...

class ChildProtocol(BaseProtocol, Protocol):
    marker: Any

    @property
    def value(self) -> str: ...

static_assert(is_subtype_of(Top[ChildProtocol], BaseProtocol))
static_assert(is_subtype_of(ChildProtocol, Top[BaseProtocol]))
static_assert(is_subtype_of(ChildProtocol, Bottom[BaseProtocol]))
```

A covariant `Awaitable[int]` satisfies the top-materialized `Awaitable[object]` protocol. Narrowing
to that protocol must therefore preserve `Awaitable[int]` without retaining a redundant
intersection:

```py
from typing import Awaitable
from typing_extensions import TypeIs

static_assert(is_subtype_of(Awaitable[int], Top[Awaitable[object]]))

def is_top_awaitable(value: object) -> TypeIs[Top[Awaitable[object]]]:
    return True

def narrow_awaitable(value: Awaitable[int]) -> None:
    if is_top_awaitable(value):
        reveal_type(value)  # revealed: Awaitable[int]
```

### Class variables

Class variables have separate read and write types. `Top` reads `object` and writes `Never`, while
`Bottom` reads `Never` and writes `object`. These requirements are preserved on both inferred and
explicitly annotated class objects:

```py
from typing import Any, ClassVar, Protocol
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_subtype_of

class ClassVarAny(Protocol):
    value: ClassVar[Any]

def class_writes(top: Top[ClassVarAny], bottom: Bottom[ClassVarAny]) -> None:
    type(top).value = 1  # error: [invalid-assignment]
    type(bottom).value = object()

def class_reads(top: Top[ClassVarAny], bottom: Bottom[ClassVarAny]) -> None:
    reveal_type(type(top).value)  # revealed: object
    reveal_type(type(bottom).value)  # revealed: Never

def annotated_class_writes(top: type[Top[ClassVarAny]], bottom: type[Bottom[ClassVarAny]]) -> None:
    reveal_type(top)  # revealed: type[Top[ClassVarAny]]
    reveal_type(bottom)  # revealed: type[Bottom[ClassVarAny]]
    top.value = 1  # error: [invalid-assignment]
    bottom.value = object()

def annotated_class_reads(top: type[Top[ClassVarAny]], bottom: type[Bottom[ClassVarAny]]) -> None:
    reveal_type(top.value)  # revealed: object
    reveal_type(bottom.value)  # revealed: Never
```

Structural protocol checks use the mapped read and write types as well. `ClassVarInt` satisfies the
top-materialized protocol, but not the bottom-materialized one; a class missing the class variable
does not satisfy the top-materialized protocol:

```py
class ClassVarInt:
    value: ClassVar[int] = 1

class MissingClassVar: ...

static_assert(is_subtype_of(ClassVarInt, Top[ClassVarAny]))
static_assert(not is_subtype_of(ClassVarInt, Bottom[ClassVarAny]))
top_class: type[Top[ClassVarAny]] = ClassVarInt
missing_top_class: type[Top[ClassVarAny]] = MissingClassVar  # error: [invalid-assignment]
invalid_bottom_class: type[Bottom[ClassVarAny]] = ClassVarInt  # error: [invalid-assignment]

def materialized_bottom_class(bottom: Bottom[ClassVarAny]) -> None:
    valid_bottom_class: type[Bottom[ClassVarAny]] = type(bottom)
    reveal_type(valid_bottom_class)  # revealed: type[Bottom[ClassVarAny]]
```

Union simplification preserves the materialized class variable regardless of operand order:

```py
def class_union_order(
    plain: ClassVarAny,
    top: Top[ClassVarAny],
    flag: bool,
) -> None:
    plain_first = type(plain) if flag else type(top)
    top_first = type(top) if flag else type(plain)
    reveal_type(plain_first.value)  # revealed: object
    reveal_type(top_first.value)  # revealed: object
```

### Methods through the class object

Ordinary, static, and class methods use their materialized signatures when accessed through the
class object. Ordinary methods remain unbound:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class DecoratedAny(Protocol):
    def transform(self, value: Any) -> Any: ...
    @staticmethod
    def parse(value: Any) -> Any: ...
    @classmethod
    def create(cls, value: Any) -> Any: ...

def decorated_class_access(
    top: Top[DecoratedAny],
    bottom: Bottom[DecoratedAny],
) -> None:
    reveal_type(type(top).transform)  # revealed: (self, /, value: Never) -> object
    reveal_type(type(top).parse)  # revealed: (value: Never) -> object
    reveal_type(type(top).create)  # revealed: (value: Never) -> object
    reveal_type(type(bottom).transform)  # revealed: (self, /, value: object) -> Never
    reveal_type(type(bottom).parse)  # revealed: (value: object) -> Never
    reveal_type(type(bottom).create)  # revealed: (value: object) -> Never
```

### Members outside the protocol interface

`__init__` is not a protocol requirement, but accessing it on a materialized value still uses the
declaration on the protocol class:

```py
from typing import Any, Protocol
from ty_extensions import Top

class ProtocolWithInit(Protocol):
    value: Any

    def __init__(self, value: int) -> None: ...

def constructor(top: Top[ProtocolWithInit]) -> None:
    reveal_type(top.__init__)  # revealed: bound method Top[ProtocolWithInit].__init__(value: int) -> None
```

### Read-only property deletion

Materializing a read-only property must not make it deletable:

```py
from typing import Any, Protocol
from typing_extensions import TypeIs
from ty_extensions import Top

class ReadOnlyProperty(Protocol):
    @property
    def property(self) -> Any: ...

def is_read_only_property(value: object) -> TypeIs[Top[ReadOnlyProperty]]:
    return True

def property_deletion(
    top: Top[ReadOnlyProperty],
    value: object,
) -> None:
    del top.property  # error: [invalid-assignment]
    if is_read_only_property(value):
        del value.property  # error: [invalid-assignment]
```

### Descriptor-decorated properties

A descriptor can expose separate read and write types. `Top` maps an `Any` read to `object` and an
`Any` write to `Never`; `Bottom` maps them in the opposite direction:

```py
from typing import Any, Callable, Never, Protocol
from typing_extensions import TypeIs
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_subtype_of

class Descriptor:
    def __get__(self, instance: object, owner: type[object] | None = None) -> Any: ...
    def __set__(self, instance: object, value: Any) -> None: ...

def descriptor(function: Callable[..., Any]) -> Descriptor:
    raise NotImplementedError

class DescriptorProperty(Protocol):
    @descriptor
    def value(self) -> Any: ...

class TopDescriptorProperty:
    @property
    def value(self) -> object:
        return object()

    @value.setter
    def value(self, value: Never) -> None: ...

class NarrowBottomDescriptorProperty:
    @property
    def value(self) -> Never:
        raise RuntimeError

    @value.setter
    def value(self, value: int) -> None: ...

static_assert(is_subtype_of(TopDescriptorProperty, Top[DescriptorProperty]))
static_assert(not is_subtype_of(NarrowBottomDescriptorProperty, Bottom[DescriptorProperty]))

def top_descriptor_write(top: Top[DescriptorProperty]) -> None:
    top.value = 1  # error: [invalid-assignment]

def bottom_descriptor_write(bottom: Bottom[DescriptorProperty]) -> None:
    bottom.value = object()

def plain_descriptor_write(plain: DescriptorProperty) -> None:
    plain.value = object()

def is_descriptor_property(value: object) -> TypeIs[Top[DescriptorProperty]]:
    return True

def narrowed_descriptor_write(value: object) -> None:
    if is_descriptor_property(value):
        reveal_type(value)  # revealed: Top[DescriptorProperty]
        value.value = 1  # error: [invalid-assignment]
```

### Property accessor types

Materializing a property with fully static exposed types is a no-op. The accessor's implicit
receiver and the setter's return type do not contribute to the property requirement:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class FullyStaticProperty(Protocol):
    @property
    def value(self) -> int: ...
    @value.setter
    def value(self, value: int) -> Any: ...

def fully_static_property(
    top: Top[FullyStaticProperty],
    bottom: Bottom[FullyStaticProperty],
) -> None:
    reveal_type(top)  # revealed: FullyStaticProperty
    reveal_type(bottom)  # revealed: FullyStaticProperty
```

### Assignment narrowing of materialized properties

A materialized protocol exposes a property's return type, not the underlying descriptor, when
reading that property. Assignment narrowing must still recover the descriptor: its setter can
transform the assigned value, so the next read must not narrow to the assigned literal.

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class TransformingProperty(Protocol):
    marker: Any

    @property
    def value(self) -> int: ...
    @value.setter
    def value(self, value: int) -> None: ...

def materialized_property_assignment_narrowing(
    top: Top[TransformingProperty],
    bottom: Bottom[TransformingProperty],
) -> None:
    top.value = 1
    reveal_type(top.value)  # revealed: int
    bottom.value = 2
    reveal_type(bottom.value)  # revealed: int
```

### Generic inference through inherited and structural protocols

Generic inference uses a member's materialized type, not its original `Any`. This applies both to
inherited members and to the finite requirements of independently declared structural protocols.
Bounds, constraints, and invariant requirements must still reject an incompatible materialized
member instead of accepting an invalid call or selecting the wrong overload:

```py
from typing import Any, Literal, Protocol, TypeVar, overload
from ty_extensions import Top

class InferenceBase[T](Protocol):
    @property
    def item(self) -> T: ...

class InheritedInferenceAny(InferenceBase[Any], Protocol):
    marker: Any

class StructuralInferenceAny(Protocol):
    @property
    def item(self) -> Any: ...

def infer_item[T](value: InferenceBase[T]) -> T:
    raise NotImplementedError

def materialized_inference(inherited: Top[InheritedInferenceAny]) -> None:
    reveal_type(infer_item(inherited))  # revealed: object

def materialized_structural_inference(structural: Top[StructuralInferenceAny]) -> None:
    reveal_type(infer_item(structural))  # revealed: object
```

A top-materialized structural protocol nested inside a contravariant class supplies an upper bound
without widening a narrower argument.

```py
class Contravariant[T]:
    def put(self, value: T) -> None: ...

def infer_contravariant_item[T](container: Contravariant[InferenceBase[T]], value: T) -> T:
    return value

def nested_inference(container: Contravariant[Top[StructuralInferenceAny]], value: bool) -> None:
    reveal_type(infer_contravariant_item(container, value))  # revealed: bool
```

Bounds and constraints still reject a materialized `object` property when its type is incompatible.

```py
def bounded_item[T: str](value: InferenceBase[T]) -> T:
    raise NotImplementedError

def union_bounded_item[T: str | bytes](value: InferenceBase[T]) -> T:
    raise NotImplementedError

def constrained_item[T: (str, bytes)](value: InferenceBase[T]) -> T:
    raise NotImplementedError

LegacyConstrained = TypeVar("LegacyConstrained", str, bytes)

def legacy_constrained_item(value: InferenceBase[LegacyConstrained]) -> LegacyConstrained:
    raise NotImplementedError

def invalid_materialized_bounds(
    inherited: Top[InheritedInferenceAny],
    structural: Top[StructuralInferenceAny],
) -> None:
    bounded_item(inherited)  # error: [invalid-argument-type]
    bounded_item(structural)  # error: [invalid-argument-type]
    union_bounded_item(inherited)  # error: [invalid-argument-type]
    union_bounded_item(structural)  # error: [invalid-argument-type]
    constrained_item(inherited)  # error: [invalid-argument-type]
    constrained_item(structural)  # error: [invalid-argument-type]
    legacy_constrained_item(inherited)  # error: [invalid-argument-type]
    legacy_constrained_item(structural)  # error: [invalid-argument-type]

def consistent_item[T](value: InferenceBase[T], values: list[T]) -> T:
    raise NotImplementedError

def invalid_materialized_invariant_arguments(
    inherited: Top[InheritedInferenceAny],
    structural: Top[StructuralInferenceAny],
    values: list[int],
) -> None:
    consistent_item(inherited, values)  # error: [invalid-argument-type]
    consistent_item(structural, values)  # error: [invalid-argument-type]

class InvariantInferenceBase[T](Protocol):
    item: T

class InheritedInvariantAny(InvariantInferenceBase[Any], Protocol):
    marker: Any

class StructuralInvariantAny(Protocol):
    item: Any

def invariant_item[T](value: InvariantInferenceBase[T], required: T) -> T:
    raise NotImplementedError

def invalid_materialized_invariant_members(
    inherited: Top[InheritedInvariantAny],
    structural: Top[StructuralInvariantAny],
) -> None:
    invariant_item(inherited, "required")  # error: [invalid-argument-type]
    invariant_item(structural, "required")  # error: [invalid-argument-type]

@overload
def select_item[T: str](value: InferenceBase[T]) -> Literal["bounded"]: ...
@overload
def select_item(value: object) -> Literal["fallback"]: ...
def select_item(value: object) -> Literal["bounded", "fallback"]:
    return "fallback"

@overload
def select_specific_item(value: InferenceBase[str]) -> Literal["str"]: ...
@overload
def select_specific_item(value: InferenceBase[bytes]) -> Literal["bytes"]: ...
def select_specific_item(value: object) -> str:
    raise NotImplementedError

def materialized_overload_resolution(
    inherited: Top[InheritedInferenceAny],
    structural: Top[StructuralInferenceAny],
    valid: InferenceBase[str],
) -> None:
    reveal_type(select_item(inherited))  # revealed: Literal["fallback"]
    reveal_type(select_item(structural))  # revealed: Literal["fallback"]
    reveal_type(select_item(valid))  # revealed: Literal["bounded"]
    select_specific_item(inherited)  # error: [no-matching-overload]
    select_specific_item(structural)  # error: [no-matching-overload]

@overload
def select_consistent_item[T](value: InferenceBase[T], values: list[T]) -> T: ...
@overload
def select_consistent_item(value: object, values: list[int]) -> object: ...
def select_consistent_item(value: object, values: object) -> object:
    raise NotImplementedError

def materialized_invariant_overload_resolution(
    inherited: Top[InheritedInferenceAny],
    structural: Top[StructuralInferenceAny],
    valid: InferenceBase[int],
    values: list[int],
) -> None:
    reveal_type(select_consistent_item(inherited, values))  # revealed: object
    reveal_type(select_consistent_item(structural, values))  # revealed: object
    reveal_type(select_consistent_item(valid, values))  # revealed: int
```

### Generic inference through recursive structural protocols

A recursive protocol requirement must not cause inference to discard a structurally matching
protocol's materialization. The nonrecursive property establishes the correct specialization without
expanding the recursive property.

```py
from __future__ import annotations

from typing import Any, Literal, Protocol, overload
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_subtype_of

class RecursiveValue[T](Protocol):
    @property
    def value(self) -> T: ...
    @property
    def child(self) -> RecursiveValue[T]: ...

class RecursiveAny(Protocol):
    @property
    def value(self) -> Any: ...
    @property
    def child(self) -> RecursiveAny: ...

static_assert(is_subtype_of(Top[RecursiveAny], RecursiveValue[object]))
static_assert(not is_subtype_of(Top[RecursiveAny], RecursiveValue[str]))
static_assert(is_subtype_of(Bottom[RecursiveAny], RecursiveValue[str]))
```

Inference preserves both materialization polarities: the top-materialized property infers `object`,
while the bottom-materialized property infers `Never`.

```py
def infer_recursive_value[T](value: RecursiveValue[T]) -> T:
    raise NotImplementedError

def recursive_materialized_inference(
    top: Top[RecursiveAny],
    bottom: Bottom[RecursiveAny],
    valid: RecursiveValue[str],
) -> None:
    reveal_type(top.value)  # revealed: object
    reveal_type(infer_recursive_value(top))  # revealed: object
    reveal_type(infer_recursive_value(valid))  # revealed: str
    reveal_type(infer_recursive_value(bottom))  # revealed: Never
```

A materialized recursive protocol nested inside a contravariant class contributes an upper bound
without expanding its recursive property.

```py
class Contravariant[T]:
    def put(self, value: T) -> None: ...

def infer_contravariant_value[T](container: Contravariant[RecursiveValue[T]], value: T) -> T:
    return value

def nested_recursive_inference(container: Contravariant[Top[RecursiveAny]], value: bool) -> None:
    reveal_type(infer_contravariant_value(container, value))  # revealed: bool
```

When a materialized protocol has no nonrecursive members, inference must defer to a separate
argument rather than expand its recursive requirement or reject the call.

```py
class RecursiveOnlyTarget[T](Protocol):
    @property
    def child(self) -> RecursiveOnlyTarget[T]: ...

class RecursiveOnlySource[T](Protocol):
    @property
    def child(self) -> RecursiveOnlySource[T]: ...

def infer_recursive_only[T](value: RecursiveOnlyTarget[T], witness: T) -> T:
    return witness

def infer_contravariant_recursive_only[T](value: Contravariant[RecursiveOnlyTarget[T]], witness: T) -> T:
    return witness

def no_finite_recursive_members(
    top: Top[RecursiveOnlySource[Any]],
    contravariant: Contravariant[Top[RecursiveOnlySource[Any]]],
    witness: bool,
) -> None:
    reveal_type(infer_recursive_only(top, witness))  # revealed: bool
    reveal_type(infer_contravariant_recursive_only(contravariant, witness))  # revealed: bool
```

The nonrecursive property is used only to infer the specialization. The complete protocol must still
be checked, so a matching `value` cannot hide an incompatible `child`.

```py
class WrongRecursiveAny(Protocol):
    @property
    def value(self) -> Any: ...
    @property
    def child(self) -> int: ...

static_assert(not is_subtype_of(Top[WrongRecursiveAny], RecursiveValue[object]))

def reject_incompatible_recursive_child(wrong: Top[WrongRecursiveAny]) -> None:
    infer_recursive_value(wrong)  # error: [invalid-argument-type]
```

A top-materialized `object` cannot satisfy a `str` bound or a `str`/`bytes` constraint. An ordinary
`RecursiveValue[str]` still satisfies both.

```py
def bounded_recursive_value[T: str](value: RecursiveValue[T]) -> T:
    raise NotImplementedError

def constrained_recursive_value[T: (str, bytes)](value: RecursiveValue[T]) -> T:
    raise NotImplementedError

def recursive_materialized_bounds(
    top: Top[RecursiveAny],
    valid: RecursiveValue[str],
) -> None:
    bounded_recursive_value(top)  # error: [invalid-argument-type]
    constrained_recursive_value(top)  # error: [invalid-argument-type]
    reveal_type(bounded_recursive_value(valid))  # revealed: str
    reveal_type(constrained_recursive_value(valid))  # revealed: str
```

An invariant `list[T]` cannot narrow the materialized `object` property to `int`.

```py
def infer_recursive_with_list[T](value: RecursiveValue[T], values: list[T]) -> T:
    raise NotImplementedError

def recursive_materialized_invariant_arguments(
    top: Top[RecursiveAny],
    valid: RecursiveValue[str],
    ints: list[int],
    strings: list[str],
) -> None:
    infer_recursive_with_list(top, ints)  # error: [invalid-argument-type]
    reveal_type(infer_recursive_with_list(valid, strings))  # revealed: str
```

Overload resolution also respects the bound and the complete recursive requirement. Both an
incompatible materialized property and an incompatible child select the fallback or fail when no
fallback is available; the valid `str` specialization selects the bounded overload.

```py
@overload
def select_recursive_value[T: str](value: RecursiveValue[T]) -> Literal["bounded"]: ...
@overload
def select_recursive_value(value: object) -> Literal["fallback"]: ...
def select_recursive_value(value: object) -> Literal["bounded", "fallback"]:
    return "fallback"

@overload
def select_specific_recursive_value(value: RecursiveValue[str]) -> Literal["str"]: ...
@overload
def select_specific_recursive_value(value: RecursiveValue[bytes]) -> Literal["bytes"]: ...
def select_specific_recursive_value(value: object) -> str:
    raise NotImplementedError

def recursive_materialized_overload_resolution(
    top: Top[RecursiveAny],
    wrong: Top[WrongRecursiveAny],
    valid: RecursiveValue[str],
) -> None:
    reveal_type(select_recursive_value(top))  # revealed: Literal["fallback"]
    reveal_type(select_recursive_value(wrong))  # revealed: Literal["fallback"]
    reveal_type(select_recursive_value(valid))  # revealed: Literal["bounded"]
    select_specific_recursive_value(top)  # error: [no-matching-overload]
    select_specific_recursive_value(wrong)  # error: [no-matching-overload]
    reveal_type(select_specific_recursive_value(valid))  # revealed: Literal["str"]
```

### Generator delegation

`yield from` uses the same materialized yield and return types as direct generator methods. Applying
another materialization must not change a result that no longer contains `Any`:

```py
from collections.abc import Generator
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class MaterializedGenerator(Generator[Any, Any, Any], Protocol):
    marker: Any

def generator_delegation(
    generator: Top[MaterializedGenerator],
    nested: Bottom[Top[MaterializedGenerator]],
):
    reveal_type(generator.__next__())  # revealed: object
    result = yield from generator
    reveal_type(result)  # revealed: object
    nested_result = yield from nested
    reveal_type(nested_result)  # revealed: object
```

The send type is contravariant. A top-materialized generator cannot accept values sent by a
`Generator[object, object, object]`, while a bottom-materialized generator can:

```py
def top_generator_send(
    generator: Top[MaterializedGenerator],
) -> Generator[object, object, object]:
    result = yield from generator  # error: [invalid-yield]
    return result

def bottom_generator_send(
    generator: Bottom[MaterializedGenerator],
) -> Generator[object, object, object]:
    result = yield from generator
    return result
```

### `Self` binding

`Self` may appear in `Top[GenericProtocol[Self]]` even when the protocol member itself is `Any`. It
must still bind to the class through which the attribute is accessed:

```py
from typing import Any, Protocol, Self
from ty_extensions import Top

class GenericProtocol[T](Protocol):
    value: Any

class SelfContainer:
    member: Top[GenericProtocol[Self]]

class SelfContainerChild(SelfContainer):
    pass

reveal_type(SelfContainerChild().member)  # revealed: Top[GenericProtocol[SelfContainerChild]]
```

### Legacy type variables

A legacy type variable in the protocol's type arguments still makes the enclosing function generic:

```py
from typing import Any, Protocol, TypeVar
from ty_extensions import Top

T = TypeVar("T")

class LegacyProtocol(Protocol[T]):
    value: Any

def accepts_legacy(value: Top[LegacyProtocol[T]]) -> None: ...

reveal_type(accepts_legacy)  # revealed: def accepts_legacy[T](value: Top[LegacyProtocol[T]]) -> None
```

### Generic aliases

Expanding a generic alias preserves the materialized write type:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class GenericMutable[T](Protocol):
    value: T

type MutableAlias[T] = GenericMutable[T]

def alias_writes(
    top: Top[MutableAlias[Any]],
    bottom: Bottom[MutableAlias[Any]],
) -> None:
    top.value = 1  # error: [invalid-assignment]
    bottom.value = object()

def annotated_generic_protocol_classes(
    top: type[Top[GenericMutable[Any]]],
    bottom: type[Bottom[GenericMutable[Any]]],
    aliased_top: type[Top[MutableAlias[Any]]],
    aliased_bottom: type[Bottom[MutableAlias[Any]]],
) -> None:
    reveal_type(top)  # revealed: type[Top[GenericMutable[Any]]]
    reveal_type(bottom)  # revealed: type[Bottom[GenericMutable[Any]]]
    reveal_type(aliased_top)  # revealed: type[Top[GenericMutable[Any]]]
    reveal_type(aliased_bottom)  # revealed: type[Bottom[GenericMutable[Any]]]
```

### Nested generic protocols

A protocol nested inside another generic type preserves its separate read and write requirements
after materialization:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class Leaf[T](Protocol):
    value: T

class Outer[T](Protocol):
    leaf: Leaf[T]

class ReadHolder[T]:
    @property
    def outer(self) -> Outer[T]:
        raise NotImplementedError

def nested_specialization(
    holder: Top[ReadHolder[Any]],
    top_leaf: Top[Leaf[Any]],
    bottom_leaf: Bottom[Leaf[Any]],
) -> None:
    reveal_type(holder.outer)  # revealed: Top[Outer[Any]]
    holder.outer.leaf = bottom_leaf
    holder.outer.leaf = top_leaf  # error: [invalid-assignment]
```

### Class-backed protocol specialization during interface construction

An ordinary specialization of a class-backed protocol only maps its class specialization. It must
not inspect the protocol interface, because the specialization can occur while that same interface
is being constructed:

```py
from __future__ import annotations

from typing import Generic, Protocol, TypeVar, overload

S = TypeVar("S")
T = TypeVar("T")

class Unit(Protocol):
    def __mul__(self, other: S | Quantity[S]): ...

class Vector(Protocol): ...

class Quantity(Generic[T], Protocol):
    @overload
    def __mul__(self, other: Unit | Quantity[S]): ...
    @overload
    def __mul__(self, other: Vector) -> Vector: ...
```

### Recursive protocols

Materializing a recursive protocol preserves its wrapper without eagerly expanding its recursive
interface. Nonrecursive members are still materialized, and following the recursive child preserves
both the protocol and its materialization polarity.

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

type RecursiveAlias = RecursiveProtocol

class RecursiveProtocol(Protocol):
    marker: Any

    @property
    def child(self) -> RecursiveAlias: ...

static_assert(is_equivalent_to(Top[RecursiveProtocol], Top[Top[RecursiveProtocol]]))
static_assert(is_equivalent_to(Bottom[RecursiveProtocol], Bottom[Bottom[RecursiveProtocol]]))
static_assert(is_equivalent_to(Top[RecursiveProtocol], Bottom[Top[RecursiveProtocol]]))
static_assert(is_equivalent_to(Bottom[RecursiveProtocol], Top[Bottom[RecursiveProtocol]]))

def recursive_top_materialization(top: Top[RecursiveProtocol]) -> None:
    reveal_type(top)  # revealed: Top[RecursiveProtocol]
    reveal_type(top.marker)  # revealed: object
    top.marker = 1  # error: [invalid-assignment]

    reveal_type(top.child)  # revealed: Top[RecursiveProtocol]
    reveal_type(top.child.child)  # revealed: Top[RecursiveProtocol]
    reveal_type(top.child.marker)  # revealed: object
    top.child.marker = 1  # error: [invalid-assignment]

def recursive_bottom_children(bottom: Bottom[RecursiveProtocol]) -> None:
    reveal_type(bottom)  # revealed: Bottom[RecursiveProtocol]
    reveal_type(bottom.child)  # revealed: Bottom[RecursiveProtocol]
    reveal_type(bottom.child.child)  # revealed: Bottom[RecursiveProtocol]
    bottom.child.marker = object()
    reveal_type(bottom.child.marker)  # revealed: Never

def recursive_bottom_marker(bottom: Bottom[RecursiveProtocol]) -> None:
    bottom.marker = object()
    reveal_type(bottom.marker)  # revealed: Never

def recursive_nested_materialization(
    nested_top: Top[Top[RecursiveProtocol]],
    nested_bottom: Bottom[Bottom[RecursiveProtocol]],
) -> None:
    reveal_type(nested_top)  # revealed: Top[RecursiveProtocol]
    reveal_type(nested_top.marker)  # revealed: object
    reveal_type(nested_bottom)  # revealed: Bottom[RecursiveProtocol]
    reveal_type(nested_bottom.marker)  # revealed: Never
```

### Display

Materialized protocols display `Top` and `Bottom` around the protocol class:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class ReadAny(Protocol):
    @property
    def value(self) -> Any: ...

def _(top: Top[ReadAny], bottom: Bottom[ReadAny]) -> None:
    reveal_type(top)  # revealed: Top[ReadAny]
    reveal_type(bottom)  # revealed: Bottom[ReadAny]
```
