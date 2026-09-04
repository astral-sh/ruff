# Gradual generic inference

```toml
[environment]
python-version = "3.14"
```

## Gradual constraints

Generic calls preserve gradual types inferred from arguments. This applies both to an argument of
type `Any` and to a callable whose parameter type is `Any`.

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

A gradual argument contributes its complete type as a lower bound on `T`, including any static union
or intersection members.

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

Callable parameters are contravariant, so their gradual types instead contribute upper bounds on
`T`.

```py
from typing import Callable

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

## Gradual arguments to structured parameters

Assigning `Any` to `tuple[T]` requires a tuple materialization, but does not determine its element
type. We infer `Any` for `T`.

```py
from typing import Any, Callable

def takes_tuple[T](value: tuple[T]) -> T:
    raise NotImplementedError

def _(value: Any):
    reveal_type(takes_tuple(value))  # revealed: Any
```

Conversely, assigning `tuple[T]` to `Any` through a callable parameter produces a gradual upper
bound on `T`.

```py
def takes_tuple_callable[T](callable: Callable[[tuple[T]], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...

reveal_type(takes_tuple_callable(accepts_any))  # revealed: Any
```

## Optional unions

A gradual argument can satisfy `T | None` by materializing to `None`, without constraining `T`. This
does not erase the gradual bound inferred from the `T` alternative.

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

## Bounded type variables

Declared type-variable bounds do not yet restrict gradual types inferred from arguments.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def takes_tuple[T](value: tuple[T]) -> T:
    raise NotImplementedError

def takes_bounded[T: int](value: T) -> T:
    return value

def takes_bounded_tuple[T: tuple[int]](value: T) -> T:
    return value

def _(x: Any):
    reveal_type(takes_tuple(x))  # revealed: Any
    # TODO: This should reveal `Any & int`.
    reveal_type(takes_bounded(takes_tuple(x)))  # revealed: Any
    # TODO: This should reveal `Any & tuple[int]`.
    reveal_type(takes_bounded_tuple(takes_tuple(x)))  # revealed: Any

def _(x: Unknown):
    reveal_type(takes_tuple(x))  # revealed: Unknown
    # TODO: This should reveal `Unknown & int`.
    reveal_type(takes_bounded(takes_tuple(x)))  # revealed: Unknown
    # TODO: This should reveal `Unknown & tuple[int]`.
    reveal_type(takes_bounded_tuple(takes_tuple(x)))  # revealed: Unknown
```

When inference comes from a callable parameter, we do preserve the declared upper bound.

```py
def takes_bounded_callable[T: int](callable: Callable[[T], None]) -> T:
    raise NotImplementedError

def takes_bounded_tuple_callable[T: tuple[int]](callable: Callable[[T], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...
def accepts_unknown(value: Unknown): ...
def _():
    reveal_type(takes_bounded_callable(accepts_any))  # revealed: Any & int
    reveal_type(takes_bounded_tuple_callable(accepts_any))  # revealed: Any & tuple[int]
    reveal_type(takes_bounded_callable(accepts_unknown))  # revealed: Unknown & int
    reveal_type(takes_bounded_tuple_callable(accepts_unknown))  # revealed: Unknown & tuple[int]
```

## Defaulted type variables

A gradual argument overrides a type variable's default but does not yet preserve its declared upper
bound.

```py
from typing import Any, Callable, TypeVar
from ty_extensions._internal import Unknown

DefaultFloat = TypeVar("DefaultFloat", bound=float, default=float)

def takes_default(x: DefaultFloat | tuple[DefaultFloat]) -> DefaultFloat:
    raise NotImplementedError

def takes_optional_default(x: DefaultFloat | None) -> DefaultFloat:
    raise NotImplementedError

def _(x: Any):
    # TODO: This should reveal `Any & float`.
    reveal_type(takes_default(x))  # revealed: Any
    # TODO: This should reveal `Any & float`.
    reveal_type(takes_optional_default(x))  # revealed: Any

def _(x: Unknown):
    # TODO: This should reveal `Unknown & float`.
    reveal_type(takes_default(x))  # revealed: Unknown
    # TODO: This should reveal `Unknown & float`.
    reveal_type(takes_optional_default(x))  # revealed: Unknown
```

The bound is preserved when inference instead comes from a callable parameter.

```py
DefaultStr = TypeVar("DefaultStr", bound=str, default=str)

def takes_default_callable(
    callable: Callable[[DefaultStr | tuple[DefaultStr]], None],
) -> DefaultStr:
    raise NotImplementedError

def takes_optional_default_callable(
    callable: Callable[[DefaultStr | None], None],
) -> DefaultStr:
    raise NotImplementedError

def accepts_any(value: Any): ...
def accepts_unknown(value: Unknown): ...
def _():
    reveal_type(takes_default_callable(accepts_any))  # revealed: Any & str
    reveal_type(takes_optional_default_callable(accepts_any))  # revealed: Any & str
    reveal_type(takes_default_callable(accepts_unknown))  # revealed: Unknown & str
    reveal_type(takes_optional_default_callable(accepts_unknown))  # revealed: Unknown & str
```

## Gradual bounds on multiple type variables

Each inferable variable in a union retains the gradual argument, whether the union appears inside a
tuple or combines different parameter shapes.

```py
from collections.abc import Iterable
from typing import Any, Callable
from ty_extensions._internal import Unknown

def takes_union_element[T, U](value: tuple[T | U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_union_arms[T, U](value: T | tuple[U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_iterable_union[T](x: T | Iterable[T]) -> T:
    raise NotImplementedError

def _(x: Any):
    reveal_type(takes_union_element(x))  # revealed: tuple[Any, Any]
    reveal_type(takes_union_arms(x))  # revealed: tuple[Any, Any]
    reveal_type(takes_iterable_union(x))  # revealed: Any

def _(x: Unknown):
    reveal_type(takes_union_element(x))  # revealed: tuple[Unknown, Unknown]
    reveal_type(takes_union_arms(x))  # revealed: tuple[Unknown, Unknown]
    reveal_type(takes_iterable_union(x))  # revealed: Unknown
```

Callable parameters contribute the corresponding gradual upper bounds.

```py
def takes_union_element_callable[T, U](callable: Callable[[tuple[T | U]], None]) -> tuple[T, U]:
    raise NotImplementedError

def takes_union_arms_callable[T, U](callable: Callable[[T | tuple[U]], None]) -> tuple[T, U]:
    raise NotImplementedError

def takes_iterable_union_callable[T](callable: Callable[[T | Iterable[T]], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...
def accepts_unknown(value: Unknown): ...
def _():
    reveal_type(takes_union_element_callable(accepts_any))  # revealed: tuple[Any, Any]
    reveal_type(takes_union_arms_callable(accepts_any))  # revealed: tuple[Any, Any]
    reveal_type(takes_iterable_union_callable(accepts_any))  # revealed: Any
    reveal_type(takes_union_element_callable(accepts_unknown))  # revealed: tuple[Unknown, Unknown]
    reveal_type(takes_union_arms_callable(accepts_unknown))  # revealed: tuple[Unknown, Unknown]
    reveal_type(takes_iterable_union_callable(accepts_unknown))  # revealed: Unknown
```

## Additional argument bounds

A concrete fallback contributes a lower bound without erasing the gradual bound inferred from a
structured argument.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def takes_tuple_with_fallback[T](value: tuple[T], fallback: T) -> T:
    return fallback

def _(x: Any):
    reveal_type(takes_tuple_with_fallback(x, 1))  # revealed: Any | Literal[1]

def _(x: Unknown):
    reveal_type(takes_tuple_with_fallback(x, 1))  # revealed: Unknown | Literal[1]
```

A callable that accepts `int` restricts the gradual result to `int & Any`.

```py
def takes_tuple_with_upper[T](value: tuple[T], upper: Callable[[T], None]) -> T:
    raise NotImplementedError

def _(x: Any, upper: Callable[[int], None]):
    reveal_type(takes_tuple_with_upper(x, upper))  # revealed: int & Any

def _(x: Unknown, upper: Callable[[int], None]):
    reveal_type(takes_tuple_with_upper(x, upper))  # revealed: int & Unknown
```

## Recursive protocols

Inference from a gradual argument can fall back to `Unknown` when the protocol has a
self-referential member.

```py
from typing import Any, Callable, Protocol
from ty_extensions._internal import Unknown

class RecursiveProtocol[T](Protocol):
    def item(self) -> T | "RecursiveProtocol[T]": ...

def takes_recursive_protocol[T](value: RecursiveProtocol[T]) -> T:
    raise NotImplementedError

def _(x: Any):
    # TODO: This should reveal `Any`.
    reveal_type(takes_recursive_protocol(x))  # revealed: Unknown

def _(x: Unknown):
    reveal_type(takes_recursive_protocol(x))  # revealed: Unknown
```

The same limitation applies when the recursive protocol appears in a callable parameter.

```py
def takes_recursive_protocol_callable[T](
    callable: Callable[[RecursiveProtocol[T]], None],
) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...
def accepts_unknown(value: Unknown): ...
def _():
    # TODO: This should reveal `Any`.
    reveal_type(takes_recursive_protocol_callable(accepts_any))  # revealed: Unknown
    reveal_type(takes_recursive_protocol_callable(accepts_unknown))  # revealed: Unknown
```

## Structural and invariant types

Structural matching preserves the gradual type of a protocol attribute. Invariant parameters
preserve the gradual argument itself.

```py
from typing import Any, Callable, Protocol
from ty_extensions._internal import Unknown

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

def _(x: Any):
    reveal_type(unbox(AnyBox()))  # revealed: Any
    reveal_type(takes_invariant(x))  # revealed: Any

def _(x: Unknown):
    reveal_type(takes_invariant(x))  # revealed: Unknown
```

The same types contribute gradual upper bounds when they occur in callable parameters.

```py
def takes_box_callable[T](callable: Callable[[Box[T]], None]) -> T:
    raise NotImplementedError

def takes_invariant_callable[T](callable: Callable[[Invariant[T]], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...
def accepts_unknown(value: Unknown): ...
def _():
    reveal_type(takes_box_callable(accepts_any))  # revealed: Any
    reveal_type(takes_invariant_callable(accepts_any))  # revealed: Any
    reveal_type(takes_box_callable(accepts_unknown))  # revealed: Unknown
    reveal_type(takes_invariant_callable(accepts_unknown))  # revealed: Unknown
```

## Callable variance

A gradual consumer does not widen a concrete argument. A gradual producer instead contributes a
gradual lower bound alongside the concrete fallback.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def consume[T](callback: Callable[[T], int], value: T) -> T:
    return value

def produce[T](callback: Callable[[], T], fallback: T) -> T:
    return fallback

def _(x: Any):
    reveal_type(consume(x, 1))  # revealed: Literal[1]
    reveal_type(produce(x, 1))  # revealed: Any | Literal[1]

def _(x: Unknown):
    reveal_type(consume(x, 1))  # revealed: Literal[1]
    reveal_type(produce(x, 1))  # revealed: Unknown | Literal[1]
```

We also preserve gradual bounds when a callable parameter is itself a callable.

```py
def takes_consumer_callable[T](callable: Callable[[Callable[[T], int]], None]) -> T:
    raise NotImplementedError

def takes_producer_callable[T](callable: Callable[[Callable[[], T]], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any): ...
def accepts_unknown(value: Unknown): ...
def _():
    reveal_type(takes_consumer_callable(accepts_any))  # revealed: Any
    reveal_type(takes_producer_callable(accepts_any))  # revealed: Any
    reveal_type(takes_consumer_callable(accepts_unknown))  # revealed: Unknown
    reveal_type(takes_producer_callable(accepts_unknown))  # revealed: Unknown
```

## Outer type variables

A gradual callback constrains its local type variable without discarding an enclosing type variable.

```py
from typing import Any, Callable

def _[S](callback: Any, value: S):
    def inner[T](callback: Callable[[T], S], value: T) -> tuple[T, S]:
        raise NotImplementedError

    reveal_type(inner(callback, value))  # revealed: tuple[S@_ | Any, S@_]
```

## Bounds from structured gradual arguments

Union members contribute constraints together, while intersection members can independently satisfy
a parameter type. For example, `Iterable[bool] | (Any & Iterable[int])` satisfies `Iterable[T]` with
both `T = bool | Any` and `T = int`. The result satisfies both return types, so we infer their
intersection, `bool | (Any & int)`.

```py
from collections.abc import Iterable
from typing import Any, Callable

def takes_iterable[T](value: Iterable[T]) -> T:
    raise NotImplementedError

def _(
    lower_bounded: Iterable[str] | Any,
    upper_bounded: Any & Iterable[int],
    bounded: Iterable[bool] | (Any & Iterable[int]),
):
    reveal_type(takes_iterable(lower_bounded))  # revealed: str | Any
    reveal_type(takes_iterable(upper_bounded))  # revealed: Any & int
    reveal_type(takes_iterable(bounded))  # revealed: bool | (Any & int)
```

These bounds also apply when inference comes from a callable parameter.

```py
def takes_iterable_callable[T](callable: Callable[[Iterable[T]], None]) -> T:
    raise NotImplementedError

def accepts_lower_bounded_iterable(value: Iterable[str] | Any): ...
def accepts_upper_bounded_iterable(value: Any & Iterable[int]): ...
def accepts_bounded_iterable(value: Iterable[bool] | (Any & Iterable[int])): ...
def _():
    reveal_type(takes_iterable_callable(accepts_lower_bounded_iterable))  # revealed: str | Any
    reveal_type(takes_iterable_callable(accepts_upper_bounded_iterable))  # revealed: Any & int
    reveal_type(takes_iterable_callable(accepts_bounded_iterable))  # revealed: bool | (Any & int)
```

If every alternative fixes `T` to `int`, we infer `int` even when another tuple element varies.

```py
def takes_stable_source[T](value: tuple[T, object]) -> T:
    raise NotImplementedError

def takes_stable_target[T](callable: Callable[[tuple[T, str]], None]) -> T:
    raise NotImplementedError

def accepts_stable_target(value: tuple[int, str] | (Any & tuple[int, object])): ...
def _(stable: tuple[int, str] | (Any & tuple[int, object])):
    reveal_type(takes_stable_source(stable))  # revealed: int
    reveal_type(takes_stable_target(accepts_stable_target))  # revealed: int
```

## Bounds from gradual callable arguments

Callable parameter types contribute upper bounds rather than lower bounds.

```py
from typing import Any, Callable

def takes_callable[T](callable: Callable[[T], None]) -> T:
    raise NotImplementedError

def _(
    lower_bounded: Callable[[str], None] | Any,
    upper_bounded: Any & Callable[[int], None],
    bounded: Callable[[int], None] | (Any & Callable[[bool], None]),
):
    reveal_type(takes_callable(lower_bounded))  # revealed: str & Any
    reveal_type(takes_callable(upper_bounded))  # revealed: Any & int
    reveal_type(takes_callable(bounded))  # revealed: Any & bool
```

A nested callable reverses the variance again. We infer a lower bound from its parameter type, but
do not yet preserve every gradual restriction.

```py
def takes_consumer_callable[T](callable: Callable[[Callable[[T], None]], None]) -> T:
    raise NotImplementedError

def accepts_lower_bounded_callable(value: Callable[[str], None] | Any): ...
def accepts_upper_bounded_callable(value: Any & Callable[[int], None]): ...
def accepts_bounded_callable(value: Callable[[int], None] | (Any & Callable[[bool], None])): ...
def _():
    # TODO: Preserve the bounds through the nested callable's parameter type.
    reveal_type(takes_consumer_callable(accepts_lower_bounded_callable))  # revealed: str | Any
    reveal_type(takes_consumer_callable(accepts_upper_bounded_callable))  # revealed: Any | int
    reveal_type(takes_consumer_callable(accepts_bounded_callable))  # revealed: int | Any
```

## Top-materialized invariant arguments

`Top[list[Unknown]]` does not constrain the element type. Intersecting it with `Any` still permits
every specialization of `list[T]`.

```py
from typing import Any
from ty_extensions import Top
from ty_extensions._internal import Unknown

def takes_list[T](value: list[T]) -> T:
    raise NotImplementedError

def _(value: Any & Top[list[Unknown]]):
    reveal_type(takes_list(value))  # revealed: Any
```

## Repeated gradual arguments

Repeated arguments contribute gradual bounds even when they have the same parameter type.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def repeated[T](values: tuple[tuple[T] | None, tuple[T] | None], lower: T) -> T:
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
    reveal_type(repeated((unknown, gradual), lower))  # revealed: Unknown | int
    reveal_type(repeated_upper((accepts_any, accepts_any)))  # revealed: Any
    reveal_type(repeated_upper((accepts_unknown, accepts_unknown)))  # revealed: Unknown
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

## Correlated tuple elements

An intersection constrains the complete return type, preserving relationships between tuple
elements. Adding `None` to the parameter does not change the bounds inferred from the tuple.

```py
from typing import Any
from ty_extensions._internal import Unknown

def takes_pair[T, U](value: tuple[T, U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_optional_pair[T, U](value: tuple[T, U] | None) -> tuple[T, U]:
    raise NotImplementedError

def _(
    pair: tuple[str, int] | Any,
    bounded_pair: tuple[str, bool] | (Any & tuple[str, int]),
    optional_bounded_pair: tuple[str, bool] | (Any & tuple[str, int]) | None,
    unknown_bounded_pair: tuple[str, bool] | (Unknown & tuple[str, int]) | None,
):
    reveal_type(takes_pair(pair))  # revealed: tuple[str | Any, int | Any]
    reveal_type(takes_pair(bounded_pair))  # revealed: tuple[str | Any, bool | Any] & tuple[str, int]
    reveal_type(takes_optional_pair(bounded_pair))  # revealed: tuple[str | Any, bool | Any] & tuple[str, int]
    reveal_type(takes_optional_pair(optional_bounded_pair))  # revealed: tuple[str | Any, bool | Any] & tuple[str, int]
    reveal_type(takes_optional_pair(unknown_bounded_pair))  # revealed: tuple[str | Unknown, bool | Unknown] & tuple[str, int]
```

## Gradual unions and intersections

A static union alternative contributes to `T` unless it already satisfies another part of the
parameter type. In particular, passing `int | Any` to `T | int` only infers `Any` for `T`.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def takes_union[T](value: T | int) -> T:
    raise NotImplementedError

def takes_nested_union[T](value: tuple[T | int]) -> T:
    raise NotImplementedError

def takes_recursive_union[T](value: T | bytes) -> T:
    raise NotImplementedError

def _(
    value: str | Any,
    lower_bounded: int | Any,
    unknown_lower_bounded: int | Unknown,
    nested: tuple[str | Any] | Any,
    recursive: Any & list[int],
):
    reveal_type(takes_union(value))  # revealed: str | Any
    reveal_type(takes_union(lower_bounded))  # revealed: Any
    reveal_type(takes_union(unknown_lower_bounded))  # revealed: Unknown
    reveal_type(takes_nested_union(nested))  # revealed: str | Any
    reveal_type(takes_recursive_union(recursive))  # revealed: Any & list[int]
```

When the parameter contains multiple type variables, each retains its own gradual bounds.

```py
def takes_union_arms[T, U](value: T | tuple[U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_union_arms_callable[T, U](callable: Callable[[T | tuple[U]], None]) -> tuple[T, U]:
    raise NotImplementedError

def takes_intersection_arms[T, U](value: T & tuple[U]) -> tuple[T, U]:
    raise NotImplementedError

def takes_intersection_arms_callable[T, U](callable: Callable[[T & tuple[U]], None]) -> tuple[T, U]:
    raise NotImplementedError

def accepts_lower_bounded(value: int | Any): ...
def _(lower_bounded: int | Any, upper_bounded: int & Any):
    reveal_type(takes_union_arms(lower_bounded))  # revealed: tuple[int | Any, Any]
    reveal_type(takes_union_arms(upper_bounded))  # revealed: tuple[int & Any, Any]
    reveal_type(takes_union_arms_callable(accepts_lower_bounded))  # revealed: tuple[int | Any, Any]
    reveal_type(takes_intersection_arms(upper_bounded))  # revealed: tuple[int & Any, Any]
    reveal_type(takes_intersection_arms_callable(accepts_lower_bounded))  # revealed: tuple[int | Any, Any]
```

## Overlapping protocol constraints

An intersection of protocols can constrain each type variable through a different member, even when
the argument also contains `Any`.

```py
from typing import Any, Callable, Protocol

class Left[T](Protocol):
    @property
    def left(self) -> T: ...

class Right[T](Protocol):
    @property
    def right(self) -> T: ...

def takes_intersection[T, U](callback: Callable[[Left[T] & Right[U]], None]) -> tuple[T, U]:
    raise NotImplementedError

def accepts_intersection(value: Any & Left[int] & Right[str]) -> None: ...

reveal_type(takes_intersection(accepts_intersection))  # revealed: tuple[int, str]

def takes_ambiguous_source[T](value: Left[T] | Right[T]) -> T:
    raise NotImplementedError
```

An object satisfying both protocols is accepted with `T = int` and with `T = str`. The generic
contract therefore requires both return types for the same object: the function cannot return on
this input. Adding `Any` to the intersection does not remove either static guarantee.

```py
def _(value: Left[int] & Right[str]):
    # TODO: Use correlated constraints for this static union comparison as well.
    reveal_type(takes_ambiguous_source(value))  # revealed: int | str

def _(value: Any & Left[int] & Right[str]):
    reveal_type(takes_ambiguous_source(value))  # revealed: Never
```

## Ambiguous structured constraints

A gradual argument can satisfy a callable parameter through different static alternatives. Neither
alternative determines the entire result.

```py
from typing import Any, Callable, Collection, Iterable, Sequence

def takes_ambiguous_invariant[T](callback: Callable[[list[T]], None]) -> T:
    raise NotImplementedError

def accepts_ambiguous_invariant(value: Any & (Iterable[int] | Sequence[str])): ...

reveal_type(takes_ambiguous_invariant(accepts_ambiguous_invariant))  # revealed: (int & Any) | (str & Any)
```

When both union alternatives contain type variables, we do not yet preserve every gradual bound.
Reversing the union's order produces the same result.

```py
def takes_ambiguous_union[T, U](value: tuple[Iterable[T] | Collection[U]]) -> tuple[T, U]:
    raise NotImplementedError

def takes_reversed_ambiguous_union[T, U](value: tuple[Collection[U] | Iterable[T]]) -> tuple[T, U]:
    raise NotImplementedError

def _(value: Any & tuple[Sequence[int]]):
    # TODO: Preserve the `Any & int` bounds in the solution.
    reveal_type(takes_ambiguous_union(value))  # revealed: tuple[Any | int, int]
    # TODO: Preserve the `Any & int` bounds in the solution.
    reveal_type(takes_reversed_ambiguous_union(value))  # revealed: tuple[Any | int, int]
```

## Type variables in gradual types

Assigning `T | Any` to `int` still constrains `T` to be a subtype of `int`. Choosing a
materialization of `Any` does not remove the `T` alternative from the union.

```py
from typing import Any, Callable

def takes_callback[T](callback: Callable[[T | Any], None]) -> T:
    raise NotImplementedError

def accepts_int(value: int) -> None: ...

reveal_type(takes_callback(accepts_int))  # revealed: int
```

The type variable can also occur inside a tuple or a type alias.

```py
type Identity[T] = T

def takes_tuple_callback[T](callback: Callable[[tuple[T] | Any], None]) -> T:
    raise NotImplementedError

def takes_alias_callback[T](callback: Callable[[Identity[T] | Any], None]) -> T:
    raise NotImplementedError

def accepts_tuple(value: tuple[int]) -> None: ...

reveal_type(takes_tuple_callback(accepts_tuple))  # revealed: int
reveal_type(takes_alias_callback(accepts_int))  # revealed: int
```

A gradual callback parameter also constrains type variables hidden inside aliases. Here, `T` can be
any materialization of `Any`, even though it occurs within a union.

```py
def takes_union_callback[T](callback: Callable[[Identity[T] | int], None]) -> T:
    raise NotImplementedError

def accepts_any(value: Any) -> None: ...

reveal_type(takes_union_callback(accepts_any))  # revealed: Any
```

In the opposite direction, an `int` return type satisfies `T & Any` only when `T` is a supertype of
`int`.

```py
def takes_producer[T](callback: Callable[[], T & Any]) -> T:
    raise NotImplementedError

def returns_int() -> int:
    return 1

reveal_type(takes_producer(returns_int))  # revealed: int
```

Bounds on `T` are also preserved when both types are gradual.

```py
def accepts_gradual_int(value: int & Any) -> None: ...
def returns_gradual_int() -> int | Any:
    return 1

reveal_type(takes_callback(accepts_gradual_int))  # revealed: int & Any
reveal_type(takes_producer(returns_gradual_int))  # revealed: int | Any
```

## Symbolic bounds in structured gradual types

The element of a tuple retains both its known alternatives and its gradual restrictions. These
bounds can contain type variables from the enclosing function without specializing those variables.

```py
from typing import Any

def first[S](value: tuple[S]) -> S:
    return value[0]

def _[T, U](value: tuple[T] | (Any & tuple[U])):
    reveal_type(first(value))  # revealed: T@_ | (Any & U@_)
```

## Overload alternatives with inferred gradual arguments

An argument of type `Any` can satisfy different overloads through different materializations. The
overloads' return types are alternatives, not simultaneous constraints on the result.

```py
from typing import Any, Callable, Never, overload

def invoke[T, R](callback: Callable[[T], R], value: T) -> R:
    return callback(value)

@overload
def callback(value: int) -> bytes: ...
@overload
def callback(value: str) -> float: ...
def callback(value: int | str) -> bytes | float:
    return b"" if isinstance(value, int) else 0.0

def _(value: Any):
    reveal_type(invoke(callback, value))  # revealed: bytes | float
```

A symbolic union member does not make the `Any` member satisfy both overloads at once. Even when the
symbolic member is bounded by `Never`, the gradual member can still materialize to either overload's
argument type.

```py
def _[U: Never](value: Any | U):
    reveal_type(invoke(callback, value))  # revealed: bytes | float

def _[U: int](value: Any | U):
    reveal_type(invoke(callback, value))  # revealed: bytes | float
```

## Overload alternatives with declared gradual parameters

A declared `Any` in a callback parameter can also make different overloads viable, even when another
argument provides a static specialization of `T`. The overloads' return types remain alternatives.

```py
from typing import Any, Callable, overload

def call[T, R](value: tuple[T], callback: Callable[[Any], R]) -> R:
    return callback(value[0])

@overload
def callback(value: int) -> int: ...
@overload
def callback(value: str) -> str: ...
def callback(value: int | str) -> int | str:
    return value

def _(value: Any & tuple[object]):
    reveal_type(call(value, callback))  # revealed: int | str
```

## Gradual parameters in overloaded arguments

An overloaded callback can declare gradual parameter types. Each overload may require a different
materialization of an argument typed as `Any`, so their return types remain alternatives.

```py
from typing import Any, Callable, Protocol, overload

class GradualCallback(Protocol):
    @overload
    def __call__(self, value: int | Any, /) -> bytes: ...
    @overload
    def __call__(self, value: str | Any, /) -> float: ...

@overload
def precise(value: int, /) -> bytes: ...
@overload
def precise(value: str, /) -> float: ...
def precise(value: int | str, /) -> bytes | float:
    return b"" if isinstance(value, int) else 0.0

gradual: GradualCallback = precise
```

The static callback above satisfies the protocol and can return normally. Passing it through a
generic function does not make its result `Never`. An `int` argument selects `bytes`, while an `Any`
argument still permits either return type and cannot justify an `int` return annotation.

```py
def invoke[T, R](callback: Callable[[T], R], value: T) -> R:
    return callback(value)

def _(callback: GradualCallback, value: Any) -> int:
    reveal_type(invoke(callback, 1))  # revealed: bytes
    # TODO: Infer `bytes | float`; the overloads need not apply under the same materialization.
    reveal_type(invoke(callback, value))  # revealed: Never
    # TODO: Report an invalid-return-type error for `bytes | float`.
    return invoke(callback, value)
```

## Unsatisfiable constraints

A gradual argument is rejected when none of its materializations satisfies the parameter type.
`str | Any` cannot materialize to a list because it always includes `str`.

```py
from typing import Any, Callable

def takes_list[T](value: list[T]) -> T:
    raise NotImplementedError

def takes_list_callable[T](callable: Callable[[list[T]], None]) -> T:
    raise NotImplementedError

def _(source: str | Any, target: Callable[[Any & int], None]):
    takes_list(source)  # error: [invalid-argument-type]
    takes_list_callable(target)  # error: [invalid-argument-type]
```

## Partially compatible structured bounds

A static tuple bound can constrain `T` even when another element does not satisfy the parameter. The
gradual component can materialize to a compatible tuple, so we should retain that information.

```py
from typing import Any, Callable

def takes_gradual_source[T](value: tuple[T, str]) -> T:
    raise NotImplementedError

def _(source: Any & tuple[int, object]):
    # TODO: This should reveal `Any & int`.
    reveal_type(takes_gradual_source(source))  # revealed: Any

def takes_gradual_target[T](callback: Callable[[tuple[T, str]], None]) -> T:
    raise NotImplementedError

def accepts_gradual_target(value: tuple[int, bytes] | Any): ...
def _():
    # TODO: This should reveal `Any & int`.
    reveal_type(takes_gradual_target(accepts_gradual_target))  # revealed: Any
```
