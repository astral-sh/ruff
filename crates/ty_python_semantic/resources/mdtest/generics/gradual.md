# Gradual generic inference

```toml
[environment]
python-version = "3.14"
```

## Gradual constraints

Generic calls that involve gradual argument types preserve gradual constraints, rather than
collapsing them to `true`.

```py
from typing import Any, Callable

def identity[T](value: T) -> T:
    raise NotImplementedError

def _(value: Any):
    reveal_type(identity(value))  # revealed: Any

def takes_callable[T](callable: Callable[[T], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...

reveal_type(takes_callable(accepts_any))  # revealed: Any
```

## Gradual parameter alternatives

A gradual union or intersection must not hide a type variable that can be inferred from a
non-gradual argument.

```py
from typing import Any
from ty_extensions import Intersection
from ty_extensions._internal import Unknown

def union_any[T](value: T | Any) -> T:
    raise NotImplementedError

def union_unknown[T](value: T | Unknown) -> T:
    raise NotImplementedError

def tuple_any[T](value: tuple[T] | Any) -> T:
    raise NotImplementedError

def tuple_unknown[T](value: tuple[T] | Unknown) -> T:
    raise NotImplementedError

def intersection_any[T](value: Intersection[T, Any]) -> T:
    raise NotImplementedError

def intersection_unknown[T](value: Intersection[T, Unknown]) -> T:
    raise NotImplementedError

def outer[S](value: S, values: tuple[S]):
    reveal_type(union_any(value))  # revealed: S@outer
    reveal_type(union_unknown(value))  # revealed: S@outer
    reveal_type(tuple_any(values))  # revealed: S@outer
    reveal_type(tuple_unknown(values))  # revealed: S@outer
    reveal_type(intersection_any(value))  # revealed: S@outer
    reveal_type(intersection_unknown(value))  # revealed: S@outer
```

The receiver of a generic method is also an argument, even when its annotation has a gradual
alternative.

```py
class Receiver:
    def any[T](self: T | Any) -> T:
        raise NotImplementedError

    def unknown[T](self: T | Unknown) -> T:
        raise NotImplementedError

reveal_type(Receiver().any())  # revealed: Receiver
reveal_type(Receiver().unknown())  # revealed: Receiver
```

## Bounded gradual constraints

When a gradual type is assigned to an inferable type variable `T` in covariant position, the
complete gradual type contributes a lower-bound constraint on `T`, preserving its gradual bounds.

```py
from typing import Any
from ty_extensions._internal import Unknown

def takes_bare[T](value: T) -> T:
    raise NotImplementedError

def _(
    unbounded: Any,
    lower_bounded: str | Any,
    upper_bounded: Any & int,
    bounded: bool | (Any & int),
    unknown_upper_bounded: Unknown & int,
    unknown_bounded: bool | (Unknown & int),
):
    reveal_type(takes_bare(unbounded))  # revealed: Any
    reveal_type(takes_bare(lower_bounded))  # revealed: str | Any
    reveal_type(takes_bare(upper_bounded))  # revealed: Any & int
    reveal_type(takes_bare(bounded))  # revealed: bool | (Any & int)
    reveal_type(takes_bare(unknown_upper_bounded))  # revealed: Unknown & int
    reveal_type(takes_bare(unknown_bounded))  # revealed: bool | (Unknown & int)
```

Conversely, when an inferable type variable `T` is assigned to a gradual type in contravariant
position, the complete gradual type contributes an upper-bound constraint on `T`.

```py
from typing import Any, Callable, final

def takes_callable[T](callable: Callable[[T], None]) -> T:
    raise NotImplementedError

def accepts_unbounded(value: Any): ...
def accepts_lower_bounded(value: str | Any): ...
def accepts_upper_bounded(value: Any & int): ...
def accepts_bounded(value: bool | (Any & int)): ...
def accepts_unknown_lower_bounded(value: str | Unknown): ...
def accepts_unknown_bounded(value: bool | (Unknown & int)): ...
def _():
    reveal_type(takes_callable(accepts_unbounded))  # revealed: Any
    reveal_type(takes_callable(accepts_lower_bounded))  # revealed: str | Any
    reveal_type(takes_callable(accepts_upper_bounded))  # revealed: Any & int
    reveal_type(takes_callable(accepts_bounded))  # revealed: bool | (Any & int)
    reveal_type(takes_callable(accepts_unknown_lower_bounded))  # revealed: str | Unknown
    reveal_type(takes_callable(accepts_unknown_bounded))  # revealed: bool | (Unknown & int)
```

## Complex gradual constraints

When checking an assignment of `Any` to `tuple[T]`, we assume that the gradual type materializes to
some tuple type. Under that assumption, we have the constraint `tuple[Any] <: tuple[T]`, allowing us
to preserve the gradual lower bound on `T`.

```py
from typing import Any

def takes_tuple[T](value: tuple[T]) -> T:
    raise NotImplementedError

def _(value: Any):
    reveal_type(takes_tuple(value))  # revealed: Any
```

When a union has only one arm that can contribute to inference, a gradual materialization through
another arm must not erase the informative arm's constraints.

```py
from typing import Any
from ty_extensions._internal import Unknown

def takes_optional[T](value: T | None) -> T:
    raise NotImplementedError

def takes_optional_tuple[T](value: tuple[T] | None) -> T:
    raise NotImplementedError

def _(any_value: Any, unknown_value: Unknown):
    reveal_type(takes_optional(any_value))  # revealed: Any
    reveal_type(takes_optional_tuple(any_value))  # revealed: Any
    reveal_type(takes_optional(unknown_value))  # revealed: Unknown
    reveal_type(takes_optional_tuple(unknown_value))  # revealed: Unknown
```

Conversely, when assigning `tuple[T]` to `Any`, `Any` may materialize to any tuple type, and so we
have a gradual upper bound on `T`.

```py
from typing import Callable

def takes_tuple_callable[T](callable: Callable[[tuple[T]], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...

reveal_type(takes_tuple_callable(accepts_any))  # revealed: Any
```

The same applies to any target type containing an inferable type variable.

```py
from typing import Any, Callable, Iterable, TypeVar, Protocol
from ty_extensions._internal import Unknown

DefaultFloat = TypeVar("DefaultFloat", bound=float, default=float)

def takes_tuple[T](value: tuple[T]) -> T:
    raise NotImplementedError

def takes_bounded[T: int](value: T) -> T:
    return value

def takes_bounded_tuple[T: tuple[int]](value: T) -> T:
    return value

def takes_tuple_with_fallback[T](value: tuple[T], fallback: T) -> T:
    return fallback

def takes_tuple_with_upper[T](
    value: tuple[T],
    upper: Callable[[T], None],
) -> T:
    raise NotImplementedError

def takes_union_element[T, U](value: tuple[T | U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_union_arms[T, U](value: T | tuple[U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_default(x: DefaultFloat | tuple[DefaultFloat]) -> DefaultFloat:
    raise NotImplementedError

def takes_optional_default(x: DefaultFloat | None) -> DefaultFloat:
    raise NotImplementedError

def takes_iterable_union[T](x: T | Iterable[T]) -> T:
    raise NotImplementedError

class RecursiveProtocol[T](Protocol):
    def item(self) -> T | "RecursiveProtocol[T]": ...

def takes_recursive_protocol[T](value: RecursiveProtocol[T]) -> T:
    raise NotImplementedError

class Box[T](Protocol):
    @property
    def value(self) -> T: ...

class AnyBox:
    value: Any

def unbox[T](value: Box[T]) -> T:
    raise NotImplementedError

class Invariant[T]:
    value: T

def takes_invariant[T](value: Invariant[T]) -> T:
    raise NotImplementedError

def consume[T](callback: Callable[[T], int], value: T) -> T:
    return value

def produce[T](callback: Callable[[], T], fallback: T) -> T:
    return fallback

def _(x: Any):
    reveal_type(takes_tuple(x))  # revealed: Any
    # TODO: This should reveal `Any & int`.
    reveal_type(takes_bounded(takes_tuple(x)))  # revealed: Any
    # TODO: This should reveal `Any & tuple[int]`.
    reveal_type(takes_bounded_tuple(takes_tuple(x)))  # revealed: Any
    reveal_type(takes_tuple_with_fallback(x, 1))  # revealed: Any | Literal[1]
    reveal_type(takes_union_element(x))  # revealed: tuple[Any, Any]
    reveal_type(takes_union_arms(x))  # revealed: tuple[Any, Any]
    # TODO: This should reveal `Any & float`.
    reveal_type(takes_default(x))  # revealed: Any
    # TODO: This should reveal `Any & float`.
    reveal_type(takes_optional_default(x))  # revealed: Any
    reveal_type(takes_iterable_union(x))  # revealed: Any

    # Recursive protocol expansion may still fall back to `Unknown`.
    # TODO: This should reveal `Any`.
    reveal_type(takes_recursive_protocol(x))  # revealed: Unknown

    reveal_type(unbox(AnyBox()))  # revealed: Any
    reveal_type(takes_invariant(x))  # revealed: Any
    reveal_type(consume(x, 1))  # revealed: Literal[1]
    reveal_type(produce(x, 1))  # revealed: Any | Literal[1]

def _(x: Unknown):
    reveal_type(takes_tuple(x))  # revealed: Unknown
    # TODO: This should reveal `Unknown & int`.
    reveal_type(takes_bounded(takes_tuple(x)))  # revealed: Unknown
    # TODO: This should reveal `Unknown & tuple[int]`.
    reveal_type(takes_bounded_tuple(takes_tuple(x)))  # revealed: Unknown
    reveal_type(takes_tuple_with_fallback(x, 1))  # revealed: Unknown | Literal[1]
    reveal_type(takes_union_element(x))  # revealed: tuple[Unknown, Unknown]
    reveal_type(takes_union_arms(x))  # revealed: tuple[Unknown, Unknown]
    # TODO: This should reveal `Unknown & float`.
    reveal_type(takes_default(x))  # revealed: Unknown
    # TODO: This should reveal `Unknown & float`.
    reveal_type(takes_optional_default(x))  # revealed: Unknown
    reveal_type(takes_iterable_union(x))  # revealed: Unknown
    reveal_type(takes_recursive_protocol(x))  # revealed: Unknown
    reveal_type(takes_invariant(x))  # revealed: Unknown
    reveal_type(consume(x, 1))  # revealed: Literal[1]
    reveal_type(produce(x, 1))  # revealed: Unknown | Literal[1]

def _(x: Any, upper: Callable[[int], None]):
    reveal_type(takes_tuple_with_upper(x, upper))  # revealed: int & Any

def _(x: Unknown, upper: Callable[[int], None]):
    reveal_type(takes_tuple_with_upper(x, upper))  # revealed: int & Unknown

def _[S](callback: Any, value: S):
    def inner[T](callback: Callable[[T], S], value: T) -> tuple[T, S]:
        raise NotImplementedError

    reveal_type(inner(callback, value))  # revealed: tuple[Any | S@_, S@_]
```

The same applies in the opposite direction when a type containing inferable type variables is
assigned to a gradual type.

```py
from collections.abc import Iterable
from typing import Any, Callable, Protocol, TypeVar
from ty_extensions._internal import Unknown

DefaultStr = TypeVar("DefaultStr", bound=str, default=str)

def takes_bounded_callable[T: int](callable: Callable[[T], None]) -> T:
    raise NotImplementedError

def takes_bounded_tuple_callable[T: tuple[int]](
    callable: Callable[[T], None],
) -> T:
    raise NotImplementedError

def takes_union_element_callable[T, U](
    callable: Callable[[tuple[T | U]], None],
) -> tuple[T, U]:
    raise NotImplementedError

def takes_union_arms_callable[T, U](
    callable: Callable[[T | tuple[U]], None],
) -> tuple[T, U]:
    raise NotImplementedError

def takes_iterable_union_callable[T](
    callable: Callable[[T | Iterable[T]], None],
) -> T:
    raise NotImplementedError

def takes_default_callable(
    callable: Callable[[DefaultStr | tuple[DefaultStr]], None],
) -> DefaultStr:
    raise NotImplementedError

def takes_optional_default_callable(
    callable: Callable[[DefaultStr | None], None],
) -> DefaultStr:
    raise NotImplementedError

class RecursiveProtocol[T](Protocol):
    def item(self) -> T | "RecursiveProtocol[T]": ...

def takes_recursive_protocol_callable[T](
    callable: Callable[[RecursiveProtocol[T]], None],
) -> T:
    raise NotImplementedError

class Box[T](Protocol):
    @property
    def value(self) -> T: ...

def takes_box_callable[T](callable: Callable[[Box[T]], None]) -> T:
    raise NotImplementedError

class Invariant[T]:
    value: T

def takes_invariant_callable[T](callable: Callable[[Invariant[T]], None]) -> T:
    raise NotImplementedError

def takes_consumer_callable[T](
    callable: Callable[[Callable[[T], int]], None],
) -> T:
    raise NotImplementedError

def takes_producer_callable[T](
    callable: Callable[[Callable[[], T]], None],
) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...
def accepts_unknown(value: Unknown): ...
def _():
    reveal_type(takes_bounded_callable(accepts_any))  # revealed: Any & int
    reveal_type(takes_bounded_tuple_callable(accepts_any))  # revealed: Any & tuple[int]
    reveal_type(takes_union_element_callable(accepts_any))  # revealed: tuple[Any, Any]
    reveal_type(takes_union_arms_callable(accepts_any))  # revealed: tuple[Any, Any]
    reveal_type(takes_iterable_union_callable(accepts_any))  # revealed: Any
    reveal_type(takes_default_callable(accepts_any))  # revealed: Any & str
    reveal_type(takes_optional_default_callable(accepts_any))  # revealed: Any & str

    # Recursive protocol expansion may still fall back to `Unknown`.
    # TODO: This should reveal `Any`.
    reveal_type(takes_recursive_protocol_callable(accepts_any))  # revealed: Unknown

    reveal_type(takes_box_callable(accepts_any))  # revealed: Any
    reveal_type(takes_invariant_callable(accepts_any))  # revealed: Any
    reveal_type(takes_consumer_callable(accepts_any))  # revealed: Any
    reveal_type(takes_producer_callable(accepts_any))  # revealed: Any

    reveal_type(takes_bounded_callable(accepts_unknown))  # revealed: Unknown & int
    reveal_type(takes_bounded_tuple_callable(accepts_unknown))  # revealed: Unknown & tuple[int]
    reveal_type(takes_union_element_callable(accepts_unknown))  # revealed: tuple[Unknown, Unknown]
    reveal_type(takes_union_arms_callable(accepts_unknown))  # revealed: tuple[Unknown, Unknown]
    reveal_type(takes_iterable_union_callable(accepts_unknown))  # revealed: Unknown
    reveal_type(takes_default_callable(accepts_unknown))  # revealed: Unknown & str
    reveal_type(takes_optional_default_callable(accepts_unknown))  # revealed: Unknown & str
    reveal_type(takes_recursive_protocol_callable(accepts_unknown))  # revealed: Unknown
    reveal_type(takes_box_callable(accepts_unknown))  # revealed: Unknown
    reveal_type(takes_invariant_callable(accepts_unknown))  # revealed: Unknown
    reveal_type(takes_consumer_callable(accepts_unknown))  # revealed: Unknown
    reveal_type(takes_producer_callable(accepts_unknown))  # revealed: Unknown
```

## Independent gradual and concrete bounds

A gradual materialization does not change concrete bounds on another type variable.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def independent[T, U](value: T, gradual: tuple[U] | None) -> tuple[T, U]:
    raise NotImplementedError

def _(value: int, gradual: Any, unknown: Unknown):
    reveal_type(independent(value, gradual))  # revealed: tuple[int, Any]
    reveal_type(independent(value, unknown))  # revealed: tuple[int, Unknown]
```

A concrete lower bound is unioned with a gradual lower bound, regardless of argument order.

```py
def with_lower[T](gradual: tuple[T] | None, lower: T) -> T:
    raise NotImplementedError

def reversed_lower[T](lower: T, gradual: tuple[T] | None) -> T:
    raise NotImplementedError

def _(value: int, gradual: Any, unknown: Unknown):
    reveal_type(with_lower(gradual, value))  # revealed: Any | int
    reveal_type(reversed_lower(value, gradual))  # revealed: int | Any
    reveal_type(with_lower(unknown, value))  # revealed: Unknown | int
    reveal_type(reversed_lower(value, unknown))  # revealed: int | Unknown
```

A concrete upper bound instead restricts the gradual type.

```py
def with_upper[T](gradual: tuple[T] | None, upper: Callable[[T], None]) -> T:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown, upper: Callable[[int], None]):
    reveal_type(with_upper(gradual, upper))  # revealed: int & Any
    reveal_type(with_upper(unknown, upper))  # revealed: int & Unknown
```

When both concrete bounds are present, the lower bound remains independent of the restricted gradual
type.

```py
def between_bounds[T](lower: T, value: tuple[T] | None, upper: Callable[[T], None]) -> T:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown, lower: bool, upper: Callable[[int], None]):
    reveal_type(between_bounds(lower, gradual, upper))  # revealed: bool | (int & Any)
    reveal_type(between_bounds(lower, unknown, upper))  # revealed: bool | (int & Unknown)
```

## Gradual materialization across union branches

Different union arms can depend on different materializations of the same gradual argument.

```py
from typing import Any
from ty_extensions._internal import Unknown

def distinct[T, U](value: tuple[T] | list[U] | None) -> tuple[T, U]:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown):
    reveal_type(distinct(gradual))  # revealed: tuple[Any, Any]
    reveal_type(distinct(unknown))  # revealed: tuple[Unknown, Unknown]
```

## Independent gradual arguments

Gradual arguments remain distinct even when their constraints pass through the same union shape.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def distinct[T, U](first: tuple[T] | list[T] | None, second: tuple[U] | list[U] | None) -> tuple[T, U]:
    raise NotImplementedError

def bounded[T, U](first: tuple[T] | None, second: tuple[U] | None, lower: T, upper: Callable[[U], None]) -> tuple[T, U]:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown, lower: int, upper: Callable[[str], None]):
    reveal_type(distinct(gradual, unknown))  # revealed: tuple[Any, Unknown]
    reveal_type(distinct(unknown, gradual))  # revealed: tuple[Unknown, Any]
    reveal_type(bounded(gradual, unknown, lower, upper))  # revealed: tuple[Any | int, str & Unknown]
    reveal_type(bounded(unknown, gradual, lower, upper))  # revealed: tuple[Unknown | int, str & Any]
```

Both gradual arguments can also contribute bounds to the same type variable.

```py
def shared_lower[T](first: tuple[T] | list[T] | None, second: tuple[T] | list[T] | None, lower: T) -> T:
    raise NotImplementedError

def shared_upper[T](first: tuple[T] | list[T] | None, second: tuple[T] | list[T] | None, upper: Callable[[T], None]) -> T:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown, lower: int, upper: Callable[[int], None]):
    reveal_type(shared_lower(gradual, unknown, lower))  # revealed: Any | int
    reveal_type(shared_upper(gradual, unknown, upper))  # revealed: int & Any
```

Separate occurrences can also have different concrete upper bounds.

```py
def shared[T](first: tuple[T] | None, second: tuple[T] | None) -> T:
    raise NotImplementedError

def _(
    any_int: Any & tuple[int],
    any_str: Any & tuple[str],
    unknown_int: Unknown & tuple[int],
    unknown_str: Unknown & tuple[str],
):
    reveal_type(distinct(any_int, unknown_str))  # revealed: tuple[Any & int, Unknown & str]
    reveal_type(distinct(unknown_int, any_str))  # revealed: tuple[Unknown & int, Any & str]
    reveal_type(shared(any_int, any_str))  # revealed: (Any & int) | (Any & str)
    reveal_type(shared(any_str, any_int))  # revealed: (Any & str) | (Any & int)
```

The occurrences remain independent when they materialize through different generic types.

```py
def project[T, U](first: tuple[T, U] | None, second: list[T] | None, lower: U, upper: Callable[[T], None]) -> tuple[T, U]:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown, lower: str, upper: Callable[[int], None]):
    reveal_type(project(gradual, unknown, lower, upper))  # revealed: tuple[int & Any, Any | str]
    reveal_type(project(unknown, gradual, lower, upper))  # revealed: tuple[int & Any, Unknown | str]
```

## Repeated gradual projections

Repeated projections onto the same generic type preserve each occurrence's gradual type while
combining their constraints with a concrete lower bound.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def repeated[T](
    values: tuple[tuple[T] | None, tuple[T] | None],
    lower: T,
) -> T:
    return lower

def repeated_upper[T](
    callbacks: tuple[Callable[[tuple[T] | None], None], Callable[[tuple[T] | None], None]],
) -> T:
    raise NotImplementedError

def accepts_any(value: Any) -> None: ...
def accepts_unknown(value: Unknown) -> None: ...
def _(gradual: Any, unknown: Unknown, lower: int):
    reveal_type(repeated((gradual, gradual), lower))  # revealed: Any | int
    reveal_type(repeated((unknown, unknown), lower))  # revealed: Unknown | int
    reveal_type(repeated((gradual, unknown), lower))  # revealed: Any | int
    reveal_type(repeated((unknown, gradual), lower))  # revealed: Any | int
    reveal_type(repeated_upper((accepts_any, accepts_any)))  # revealed: Any
    reveal_type(repeated_upper((accepts_unknown, accepts_unknown)))  # revealed: Unknown
```

## Concrete bounds across independent union alternatives

A concrete argument can constrain different type variables on different union branches,
independently of another argument's gradual materialization.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def lower[T, U](gradual: tuple[T] | None, concrete: tuple[T] | tuple[U]) -> tuple[T, U]:
    raise NotImplementedError

def reversed_lower[T, U](gradual: tuple[T] | None, concrete: tuple[U] | tuple[T]) -> tuple[T, U]:
    raise NotImplementedError

def reversed_lower_arguments[T, U](concrete: tuple[T] | tuple[U], gradual: tuple[T] | None) -> tuple[T, U]:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown, concrete: tuple[int]):
    reveal_type(lower(gradual, concrete))  # revealed: tuple[Any | int, int]
    reveal_type(lower(unknown, concrete))  # revealed: tuple[Unknown | int, int]
    reveal_type(reversed_lower(gradual, concrete))  # revealed: tuple[Any | int, int]
    reveal_type(reversed_lower_arguments(concrete, gradual))  # revealed: tuple[int | Any, int]
```

An unconditional gradual lower bound also preserves the independent concrete bound.

```py
def direct[T, U](gradual: T, concrete: tuple[T] | tuple[U]) -> tuple[T, U]:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown, concrete: tuple[int]):
    reveal_type(direct(gradual, concrete))  # revealed: tuple[Any | int, int]
    reveal_type(direct(unknown, concrete))  # revealed: tuple[Unknown | int, int]
```

Independent lower bounds on either type variable are preserved across both alternatives.

```py
def direct_with_lower[T, U](gradual: T, concrete: tuple[T] | tuple[U], lower: U) -> tuple[T, U]:
    raise NotImplementedError

def _(gradual: Any, concrete: tuple[int], lower: str):
    reveal_type(direct_with_lower(gradual, concrete, lower))  # revealed: tuple[Any | int, int | str]
```

The same union branches can independently establish upper bounds.

```py
def upper[T, U](gradual: tuple[T] | None, concrete: Callable[[T], None] | Callable[[U], None]) -> tuple[T, U]:
    raise NotImplementedError

def reversed_upper[T, U](gradual: tuple[T] | None, concrete: Callable[[U], None] | Callable[[T], None]) -> tuple[T, U]:
    raise NotImplementedError

def reversed_upper_arguments[T, U](concrete: Callable[[T], None] | Callable[[U], None], gradual: tuple[T] | None) -> tuple[T, U]:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown, concrete: Callable[[int], None]):
    reveal_type(upper(gradual, concrete))  # revealed: tuple[int & Any, int]
    reveal_type(upper(unknown, concrete))  # revealed: tuple[int & Unknown, int]
    reveal_type(reversed_upper(gradual, concrete))  # revealed: tuple[int & Any, int]
    reveal_type(reversed_upper_arguments(concrete, gradual))  # revealed: tuple[int & Any, int]
```

## Gradual bounds across overload alternatives

An overloaded callback's concrete result can depend on the materialization of a gradual argument.

```py
from typing import Any, Callable, Protocol, overload
from ty_extensions._internal import Unknown

class Left[T, R](Protocol):
    def combine(self, other: T, /) -> R: ...

class Right[T, R](Protocol):
    def reverse(self, other: T, /) -> R: ...

@overload
def combine[T, R](first: Left[T, R], second: T, /) -> R: ...
@overload
def combine[T, R](first: T, second: Right[T, R], /) -> R: ...
def combine(first: object, second: object, /) -> object:
    raise NotImplementedError

class Static:
    def combine(self, other: int, /) -> int:
        raise NotImplementedError

def apply[T, U, R](callback: Callable[[T, U], R], first: T, second: U) -> R:
    raise NotImplementedError

def _(gradual: Any, unknown: Unknown):
    reveal_type(apply(combine, Static(), gradual))  # revealed: Any
    reveal_type(apply(combine, Static(), unknown))  # revealed: Unknown
```

## Independent gradual upper bounds

Distinct gradual callbacks can contribute upper bounds to the same type variable without conflating
`Any` and `Unknown`.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def shared[T](first: Callable[[T], None], second: Callable[[T], None]) -> T:
    raise NotImplementedError

def with_lower[T](first: Callable[[T], None], second: Callable[[T], None], lower: T) -> T:
    raise NotImplementedError

def with_upper[T](first: Callable[[T], None], second: Callable[[T], None], upper: Callable[[T], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any) -> None: ...
def accepts_unknown(value: Unknown) -> None: ...
def accepts_int(value: int) -> None: ...
def _(lower: int):
    reveal_type(shared(accepts_any, accepts_unknown))  # revealed: Any
    reveal_type(shared(accepts_unknown, accepts_any))  # revealed: Any
    reveal_type(shared(accepts_unknown, accepts_unknown))  # revealed: Unknown
    reveal_type(with_lower(accepts_any, accepts_unknown, lower))  # revealed: int
    reveal_type(with_upper(accepts_any, accepts_unknown, accepts_int))  # revealed: Any & int
```

The same distinction applies when each gradual parameter materializes to an optional generic tuple.

```py
def projected[T](first: Callable[[tuple[T] | None], None], second: Callable[[tuple[T] | None], None]) -> T:
    raise NotImplementedError

def projected_lower[T](first: Callable[[tuple[T] | None], None], second: Callable[[tuple[T] | None], None], lower: T) -> T:
    raise NotImplementedError

def projected_upper[T](
    first: Callable[[tuple[T] | None], None],
    second: Callable[[tuple[T] | None], None],
    upper: Callable[[T], None],
) -> T:
    raise NotImplementedError

def _(lower: int):
    reveal_type(projected(accepts_any, accepts_unknown))  # revealed: Any
    reveal_type(projected(accepts_unknown, accepts_any))  # revealed: Any
    reveal_type(projected_lower(accepts_any, accepts_unknown, lower))  # revealed: int
    reveal_type(projected_upper(accepts_any, accepts_unknown, accepts_int))  # revealed: Any & int
```

Independent gradual upper bounds preserve the more specific concrete restriction.

```py
def accepts_any_int(value: Any & int) -> None: ...
def accepts_unknown_int(value: Unknown & int) -> None: ...
def accepts_any_bool(value: Any & bool) -> None: ...
def accepts_unknown_bool(value: Unknown & bool) -> None: ...
def _():
    reveal_type(shared(accepts_any_int, accepts_unknown_int))  # revealed: Any & int
    reveal_type(shared(accepts_any_int, accepts_unknown_bool))  # revealed: Any & bool
    reveal_type(shared(accepts_any_bool, accepts_unknown_int))  # revealed: Any & bool
```

## Nested gradual variance

A callback reverses variance, so a gradual parameter contributes an upper bound.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def callback[T](value: Callable[[tuple[T] | None], None], lower: T) -> T:
    raise NotImplementedError

def accepts_any(value: Any) -> None: ...
def accepts_unknown(value: Unknown) -> None: ...
def _(lower: int):
    reveal_type(callback(accepts_any, lower))  # revealed: int
    reveal_type(callback(accepts_unknown, lower))  # revealed: int
```

Two nested callbacks restore covariance, producing a gradual lower bound instead.

```py
def nested[T](callback: Callable[[Callable[[tuple[T] | None], None]], None], lower: T) -> T:
    raise NotImplementedError

def accepts_any_callback(callback: Callable[[Any], None]) -> None: ...
def accepts_unknown_callback(callback: Callable[[Unknown], None]) -> None: ...
def _(lower: int):
    reveal_type(nested(accepts_any_callback, lower))  # revealed: Any | int
    reveal_type(nested(accepts_unknown_callback, lower))  # revealed: Unknown | int
```

## Bounded complex gradual constraints

We respect the upper and lower bounds when choosing a materialization of the gradual type. For
example, an assignment of `bool | (int & Any)` to `tuple[T]` will form the constraints
`tuple[bool | (int & Any)] <: tuple[T]`, allowing the upper or lower bounds to be preserved based on
variance.

```py
from collections.abc import Iterable
from typing import Any, Callable
from ty_extensions import Top
from ty_extensions._internal import Unknown

def takes_iterable[T](value: Iterable[T]) -> T:
    raise NotImplementedError

def takes_iterable_callable[T](callable: Callable[[Iterable[T]], None]) -> T:
    raise NotImplementedError

def takes_callable[T](callable: Callable[[T], None]) -> T:
    raise NotImplementedError

def takes_consumer_callable[T](
    callable: Callable[[Callable[[T], None]], None],
) -> T:
    raise NotImplementedError

def takes_stable_source[T](value: tuple[T, object]) -> T:
    raise NotImplementedError

def takes_stable_target[T](callable: Callable[[tuple[T, str]], None]) -> T:
    raise NotImplementedError

def takes_list[T](value: list[T]) -> T:
    raise NotImplementedError

def accepts_lower_bounded_iterable(value: Iterable[str] | Any): ...
def accepts_upper_bounded_iterable(
    value: Any & Iterable[int],
): ...
def accepts_bounded_iterable(
    value: Iterable[bool] | (Any & Iterable[int]),
): ...
def accepts_lower_bounded_callable(
    value: Callable[[str], None] | Any,
): ...
def accepts_upper_bounded_callable(
    value: Any & Callable[[int], None],
): ...
def accepts_bounded_callable(
    value: Callable[[int], None] | (Any & Callable[[bool], None]),
): ...
def accepts_stable_target(
    value: tuple[int, str] | (Any & tuple[int, object]),
): ...
def _(
    lower_bounded: Iterable[str] | Any,
    upper_bounded: Any & Iterable[int],
    bounded: Iterable[bool] | (Any & Iterable[int]),
    stable: tuple[int, str] | (Any & tuple[int, object]),
):
    reveal_type(takes_iterable(lower_bounded))  # revealed: str | Any
    reveal_type(takes_iterable(upper_bounded))  # revealed: Any & int
    reveal_type(takes_iterable(bounded))  # revealed: bool | (Any & int)

    reveal_type(takes_iterable_callable(accepts_lower_bounded_iterable))  # revealed: str | Any
    reveal_type(takes_iterable_callable(accepts_upper_bounded_iterable))  # revealed: Any & int
    reveal_type(takes_iterable_callable(accepts_bounded_iterable))  # revealed: bool | (Any & int)

    reveal_type(takes_stable_source(stable))  # revealed: int
    reveal_type(takes_stable_target(accepts_stable_target))  # revealed: int

def _(
    lower_bounded: Callable[[str], None] | Any,
    upper_bounded: Any & Callable[[int], None],
    bounded: Callable[[int], None] | (Any & Callable[[bool], None]),
):
    reveal_type(takes_callable(lower_bounded))  # revealed: Any & str
    reveal_type(takes_callable(upper_bounded))  # revealed: int | Any
    reveal_type(takes_callable(bounded))  # revealed: bool | (Any & int)

    reveal_type(takes_consumer_callable(accepts_lower_bounded_callable))  # revealed: Any & str
    reveal_type(takes_consumer_callable(accepts_upper_bounded_callable))  # revealed: int | Any
    reveal_type(takes_consumer_callable(accepts_bounded_callable))  # revealed: bool | (Any & int)

def _(
    value: Any & Top[list[Unknown]],
):
    reveal_type(takes_list(value))  # revealed: Any
```

## Gradual specializations

Specializations involving gradual types respect the variance of their outer type.

```py
from typing import Any

class Producer[T]:
    def produce(self) -> T:
        raise NotImplementedError

class Consumer[T]:
    def consume(self, value: T) -> None:
        raise NotImplementedError

def takes_producer[T](value: Producer[T]) -> T:
    raise NotImplementedError

def takes_consumer[T](value: Consumer[T]) -> T:
    raise NotImplementedError

def _(
    producer_lower: Producer[str | Any],
    producer_upper: Producer[str & Any],
    consumer_lower: Consumer[str | Any],
    consumer_upper: Consumer[str & Any],
):
    reveal_type(takes_producer(producer_lower))  # revealed: str | Any
    reveal_type(takes_producer(producer_upper))  # revealed: str & Any
    reveal_type(takes_consumer(consumer_lower))  # revealed: str | Any
    reveal_type(takes_consumer(consumer_upper))  # revealed: str & Any
```

## Union and intersection gradual constraints

Gradual constraints are distributed across unions and intersections.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def takes_pair[T, U](value: tuple[T, U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_optional_pair[T, U](value: tuple[T, U] | None) -> tuple[T, U]:
    raise NotImplementedError

def takes_union[T](value: T | int) -> T:
    raise NotImplementedError

def takes_nested_union[T](value: tuple[T | int]) -> T:
    raise NotImplementedError

def takes_recursive_union[T](value: T | bytes) -> T:
    raise NotImplementedError

def takes_union_arms[T, U](value: T | tuple[U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_union_arms_callable[T, U](
    callable: Callable[[T | tuple[U]], None],
) -> tuple[T, U]:
    raise NotImplementedError

def takes_intersection_arms[T, U](value: T & tuple[U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_intersection_arms_callable[T, U](
    callable: Callable[[T & tuple[U]], None],
) -> tuple[T, U]:
    raise NotImplementedError

def accepts_lower_bounded(value: int | Any):
    pass

def _(
    pair: tuple[str, int] | Any,
    fixed_point: str | Any,
    nested: tuple[str | Any] | Any,
    recursive: Any & list[int],
    lower_bounded: int | Any,
    unknown_lower_bounded: int | Unknown,
    upper_bounded: int & Any,
    bounded_pair: tuple[str, bool] | (Any & tuple[str, int]),
    optional_bounded_pair: tuple[str, bool] | (Any & tuple[str, int]) | None,
    unknown_bounded_pair: tuple[str, bool] | (Unknown & tuple[str, int]) | None,
):
    reveal_type(takes_pair(pair))  # revealed: tuple[str | Any, int | Any]
    reveal_type(takes_pair(bounded_pair))  # revealed: tuple[str, bool | (Any & int)]
    reveal_type(takes_optional_pair(bounded_pair))  # revealed: tuple[str, bool | (Any & int)]
    reveal_type(takes_optional_pair(optional_bounded_pair))  # revealed: tuple[str, bool | (Any & int)]
    reveal_type(takes_optional_pair(unknown_bounded_pair))  # revealed: tuple[str, bool | (Unknown & int)]
    reveal_type(takes_union(fixed_point))  # revealed: str | Any
    reveal_type(takes_union(lower_bounded))  # revealed: Any
    reveal_type(takes_union(unknown_lower_bounded))  # revealed: Unknown
    reveal_type(takes_nested_union(nested))  # revealed: str | Any
    reveal_type(takes_recursive_union(recursive))  # revealed: Any & list[int]
    reveal_type(takes_union_arms(lower_bounded))  # revealed: tuple[int | Any, Any]
    reveal_type(takes_union_arms(upper_bounded))  # revealed: tuple[int & Any, Unknown]
    reveal_type(takes_union_arms_callable(accepts_lower_bounded))  # revealed: tuple[int | Any, Any]
    reveal_type(takes_intersection_arms(upper_bounded))  # revealed: tuple[int & Any, Unknown]
    reveal_type(takes_intersection_arms_callable(accepts_lower_bounded))  # revealed: tuple[int | Any, Any]
```

```py
from typing import Collection, Iterable, Sequence, Protocol

class Left[T](Protocol):
    @property
    def left(self) -> T: ...

class Right[T](Protocol):
    @property
    def right(self) -> T: ...

def takes_intersection[T, U](
    callback: Callable[[Left[T] & Right[U]], None],
) -> tuple[T, U]:
    raise NotImplementedError

def accepts_intersection(value: Any & Left[int] & Right[str]) -> None:
    pass

reveal_type(takes_intersection(accepts_intersection))  # revealed: tuple[int, str]

def takes_reversed_intersection[T, U](
    callback: Callable[[Right[U] & Left[T]], None],
) -> tuple[T, U]:
    raise NotImplementedError

def accepts_unknown_intersection(value: Unknown & Left[int] & Right[str]) -> None: ...
def accepts_reversed_intersection(value: Right[str] & Left[int] & Any) -> None: ...

reveal_type(takes_intersection(accepts_unknown_intersection))  # revealed: tuple[int, str]
reveal_type(takes_intersection(accepts_reversed_intersection))  # revealed: tuple[int, str]
reveal_type(takes_reversed_intersection(accepts_intersection))  # revealed: tuple[int, str]

def takes_ambiguous_source[T](value: Left[T] | Right[T]) -> T:
    raise NotImplementedError

def _(value: Any & Left[int] & Right[str]):
    reveal_type(takes_ambiguous_source(value))  # revealed: (Any & int) | (Any & str)

def takes_ambiguous_invariant[T](callback: Callable[[list[T]], None]) -> T:
    raise NotImplementedError

def accepts_ambiguous_invariant(
    value: Any & (Iterable[int] | Sequence[str]),
):
    pass

reveal_type(takes_ambiguous_invariant(accepts_ambiguous_invariant))  # revealed: (int & Any) | (str & Any)

def takes_ambiguous_union[T, U](
    value: tuple[Iterable[T] | Collection[U]],
) -> tuple[T, U]:
    raise NotImplementedError

def takes_reversed_ambiguous_union[T, U](
    value: tuple[Collection[U] | Iterable[T]],
) -> tuple[T, U]:
    raise NotImplementedError

def _(value: Any & tuple[Sequence[int]]):
    # TODO: Preserve the `Any & int` bounds in the solution.
    reveal_type(takes_ambiguous_union(value))  # revealed: tuple[Any | int, int]
    # TODO: Preserve the `Any & int` bounds in the solution.
    reveal_type(takes_reversed_ambiguous_union(value))  # revealed: tuple[Any | int, int]
```

## Unsatisfiable constraints

A gradual type may only materialize to a type within the range of its upper and lower bounds, and is
otherwise unsatisfiable.

```py
from typing import Any, Callable

def takes_list[T](value: list[T]) -> T:
    raise NotImplementedError

def takes_list_callable[T](callable: Callable[[list[T]], None]) -> T:
    raise NotImplementedError

def _(
    source: str | Any,
    target: Callable[[Any & int], None],
):
    takes_list(source)  # error: [invalid-argument-type]
    takes_list_callable(target)  # error: [invalid-argument-type]
```

Partial constraints from unsatisfiable upper and lower bounds should be preserved.

```py
from typing import Any, Callable

def takes_gradual_source[T](value: tuple[T, str]) -> T:
    raise NotImplementedError

def _(source: Any & tuple[int, object]):
    # TODO: This should reveal `Any & int`.
    reveal_type(takes_gradual_source(source))  # revealed: Any

def takes_gradual_target[T](callback: Callable[[tuple[T, str]], None]) -> T:
    raise NotImplementedError

def accepts_gradual_target(value: tuple[int, bytes] | Any):
    pass

def _(source: Any & tuple[int, object]):
    # TODO: This should reveal `Any & int`.
    reveal_type(takes_gradual_target(accepts_gradual_target))  # revealed: Any
```
