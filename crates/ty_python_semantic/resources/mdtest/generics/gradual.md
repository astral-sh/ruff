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
