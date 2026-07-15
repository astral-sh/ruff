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
from ty_extensions import Unknown, Bottom, Top
from ty_extensions._internal import TypeOf

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
from ty_extensions import Unknown, Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

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
from ty_extensions._internal import TypeOf

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
from ty_extensions import Unknown, Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

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
from ty_extensions import Unknown, Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

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
from ty_extensions import Unknown, Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

# ~Any is still Any, so the top materialization is object
static_assert(is_equivalent_to(Top[~Any], object))
static_assert(is_equivalent_to(Bottom[~Any], Never))

# tuple[Any, int] is in a contravariant position, so the
# top materialization is Never and the negation of it
static_assert(is_equivalent_to(Top[~tuple[Any, int]], object))
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
from ty_extensions import Unknown, Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

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
from ty_extensions import Unknown, Bottom, Top, static_assert
from ty_extensions._internal import is_subtype_of

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
from ty_extensions import Top, Bottom, static_assert
from ty_extensions._internal import is_equivalent_to

static_assert(is_equivalent_to(Top[Top[list[Any]]], Top[list[Any]]))
static_assert(is_equivalent_to(Bottom[Top[list[Any]]], Top[list[Any]]))

static_assert(is_equivalent_to(Bottom[Bottom[list[Any]]], Bottom[list[Any]]))
static_assert(is_equivalent_to(Top[Bottom[list[Any]]], Bottom[list[Any]]))
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

## Protocols

Materializing a protocol maps each member according to how it is used. Reads are covariant and
writes are contravariant.

```toml
[environment]
python-version = "3.12"
```

### Member read and write types

For a mutable `Any` attribute, `Top` reads `object` and writes `Never`; `Bottom` does the reverse:

```py
from typing import Any, Callable, Protocol
from ty_extensions import Bottom, Top

class MutableAny(Protocol):
    value: Any

def invoke[T](factory: Callable[[], T]) -> T:
    return factory()

def mutable_attributes(top: Top[MutableAny], bottom: Bottom[MutableAny]) -> None:
    reveal_type(top)  # revealed: Top[MutableAny]
    reveal_type(top.value)  # revealed: object
    reveal_type(type(top))  # revealed: type[Top[MutableAny]]
    reveal_type(type(top)())  # revealed: Top[MutableAny]
    reveal_type(invoke(type(top)))  # revealed: Top[MutableAny]
    top.value = 1  # error: [invalid-assignment]
    reveal_type(bottom)  # revealed: Bottom[MutableAny]
    reveal_type(bottom.value)  # revealed: Never
    bottom.value = object()
```

A property setter is already a write, so its parameter is mapped only once:

```py
class WritableAny(Protocol):
    @property
    def value(self) -> Any: ...
    @value.setter
    def value(self, value: Any) -> None: ...

def writable_properties(top: Top[WritableAny], bottom: Bottom[WritableAny]) -> None:
    reveal_type(top.value)  # revealed: object
    top.value = 1  # error: [invalid-assignment]
    reveal_type(bottom.value)  # revealed: Never
    bottom.value = object()
```

### Relations between a protocol and its materializations

`MutableAny` and `Top[MutableAny]` refer to the same protocol class, but they do not have the same
read and write requirements. Subtyping and union simplification must use those requirements:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_subtype_of

class MutableAny(Protocol):
    value: Any

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

### Class-side access

Materialization preserves whether a protocol member is available through the class object. Ordinary
instance attributes remain unavailable through the class object after materialization:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top

class InstanceOnlyAny(Protocol):
    value: Any

def instance_only_class_access(
    top: Top[InstanceOnlyAny],
    bottom: Bottom[InstanceOnlyAny],
) -> None:
    type(top).value  # error: [unresolved-attribute]
    type(bottom).value  # error: [unresolved-attribute]
    type(top).value = 1  # error: [invalid-assignment]
    type(bottom).value = 1  # error: [invalid-assignment]
```

Class variables have separate read and write types. `Top` reads `object` and writes `Never`, while
`Bottom` reads `Never` and writes `object`:

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
```

Class-variable relations and unions use the same mapped read and write types:

```py
class ClassVarInt:
    value: ClassVar[int] = 1

static_assert(is_subtype_of(ClassVarInt, Top[ClassVarAny]))
static_assert(not is_subtype_of(ClassVarInt, Bottom[ClassVarAny]))
classvar_top_meta: type[Top[ClassVarAny]] = ClassVarInt

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

Ordinary, static, and class methods are available through the class object and use the materialized
callable signature. Ordinary methods remain unbound:

```py
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

### Other members from the protocol class

`__init__` is not a protocol requirement, but accessing it on a materialized value still uses the
declaration on the protocol class:

```py
from typing import Any, Protocol
from typing_extensions import TypeIs
from ty_extensions import Bottom, Top

class ProtocolWithInit(Protocol):
    value: Any

    def __init__(self, value: int) -> None: ...

def constructor(top: Top[ProtocolWithInit]) -> None:
    reveal_type(top.__init__)  # revealed: bound method Top[ProtocolWithInit].__init__(value: int) -> None
```

Materialization and `TypeIs` narrowing must preserve descriptor binding for class and static
methods:

```py
class DescriptorMethods(Protocol):
    @classmethod
    def make(cls) -> int: ...
    @staticmethod
    def parse() -> str: ...

def is_descriptor_methods(value: object) -> TypeIs[DescriptorMethods]:
    return True

def descriptor_methods(
    top: Top[DescriptorMethods],
    bottom: Bottom[DescriptorMethods],
    value: object,
) -> None:
    reveal_type(top.make())  # revealed: int
    reveal_type(bottom.parse())  # revealed: str
    if is_descriptor_methods(value):
        reveal_type(value.make())  # revealed: int
```

### Properties

Materializing a read-only property must not make it deletable:

```py
from typing import Any, Protocol
from typing_extensions import TypeIs
from ty_extensions import Top

class ReadOnlyProperty(Protocol):
    @property
    def property(self) -> Any: ...

def is_read_only_property(value: object) -> TypeIs[ReadOnlyProperty]:
    return True

def property_deletion(
    top: Top[ReadOnlyProperty],
    value: object,
) -> None:
    del top.property  # error: [invalid-assignment]
    if is_read_only_property(value):
        del value.property  # error: [invalid-assignment]
```

The read and write types exposed by a descriptor-decorated property are materialized with their
respective variance:

```py
from typing import Any, Callable, Never, Protocol
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
```

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

A property setter may transform the assigned value. Assigning a literal therefore must not narrow
subsequent reads to that literal:

```py
class TransformingProperty(Protocol):
    marker: Any

    @property
    def value(self) -> int: ...
    @value.setter
    def value(self, value: int) -> None: ...

def property_assignment_narrowing(top: Top[TransformingProperty]) -> None:
    top.value = 1
    reveal_type(top.value)  # revealed: int
```

### Generic inference through inherited protocols

Generic inference uses the materialized type of an inherited member, not the original `Any`:

```py
from typing import Any, Protocol
from ty_extensions import Top

class InferenceBase[T](Protocol):
    @property
    def item(self) -> T: ...

class InheritedInferenceAny(InferenceBase[Any], Protocol):
    marker: Any

def infer_item[T](value: InferenceBase[T]) -> T:
    raise NotImplementedError

def materialized_inference(inherited: Top[InheritedInferenceAny]) -> None:
    reveal_type(infer_item(inherited))  # revealed: object
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
    reveal_type(nested.__next__())  # revealed: object
    nested_result = yield from nested
    reveal_type(nested_result)  # revealed: object

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

### `Self` and legacy type variables

`Self` may appear in `Top[GenericProtocol[Self]]` even when the protocol member itself is `Any`. It
must still bind to the class through which the attribute is accessed:

```py
from typing import Any, Protocol, Self, TypeVar
from ty_extensions import Top

class GenericProtocol[T](Protocol):
    value: Any

class SelfContainer:
    member: Top[GenericProtocol[Self]]

class SelfContainerChild(SelfContainer):
    pass

reveal_type(SelfContainerChild().member)  # revealed: Top[GenericProtocol[SelfContainerChild]]
```

A legacy type variable in the protocol's type arguments still makes the enclosing function generic:

```py
T = TypeVar("T")

class LegacyProtocol(Protocol[T]):
    value: Any

def accepts_legacy(value: Top[LegacyProtocol[T]]) -> None: ...

reveal_type(accepts_legacy)  # revealed: def accepts_legacy[T](value: Top[LegacyProtocol[T]]) -> None
```

### Aliases

Expanding a generic alias preserves the materialized write type:

```py
from typing import Any, Protocol
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_equivalent_to

class GenericMutable[T](Protocol):
    value: T

type MutableAlias[T] = GenericMutable[T]

def alias_writes(
    top: Top[MutableAlias[Any]],
    bottom: Bottom[MutableAlias[Any]],
) -> None:
    top.value = 1  # error: [invalid-assignment]
    bottom.value = object()
```

Read and write mappings use separate transformation caches when a materialized specialization is
nested inside another generic type:

```py
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

Repeated materialization has no further effect when a recursive protocol reference passes through an
alias:

```py
type RecursiveAlias = RecursiveProtocol

class RecursiveProtocol(Protocol):
    marker: Any

    @property
    def child(self) -> RecursiveAlias: ...

static_assert(is_equivalent_to(Top[RecursiveProtocol], Top[Top[RecursiveProtocol]]))
```

### Display

Materialized protocols display their polarity around the protocol class while exposing their mapped
member types:

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
