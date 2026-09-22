# Generic callables: PEP 695 syntax

```toml
[environment]
python-version = "3.12"
```

## Callables can be generic

Many items that are callable can also be generic. Generic functions are the most obvious example:

```py
from typing import Callable
from ty_extensions._internal import generic_context

def identity[T](t: T) -> T:
    return t

# revealed: ty_extensions._internal.GenericContext[T@identity]
reveal_type(generic_context(identity))
# revealed: Literal[1]
reveal_type(identity(1))

def identity2[**P, T](c: Callable[P, T]) -> Callable[P, T]:
    return c

# revealed: ty_extensions._internal.GenericContext[P@identity2, T@identity2]
reveal_type(generic_context(identity2))
# revealed: [T](t: T) -> T
reveal_type(identity2(identity))

class CallableInstance:
    def __call__(self, value: int, /) -> str:
        return str(value)

# revealed: (value: int, /) -> str
reveal_type(identity2(CallableInstance()))
```

Generic classes are another example, since you invoke the class to instantiate it:

```py
class C[T]:
    t: T  # invariant

    def __init__(self, t: T) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: C[int]
reveal_type(C(1))
```

Explicit generic receiver annotations constrain a bound method's callable type:

```py
from typing import Callable

class GenericReceiver:
    def method[T](self: T, value: T) -> T:
        return self

receiver = GenericReceiver()

# Binding adds `GenericReceiver <= T`. `T = object` satisfies that constraint, but `T = int` does
# not.
accepts_object: Callable[[object], object] = receiver.method
accepts_int: Callable[[int], int] = receiver.method  # error: [invalid-assignment]
```

The receiver must also satisfy a method type variable's declared bound or constraints:

```py
from typing import Callable

class InvalidBoundedReceiver:
    def method[T: int](self: T) -> None: ...

class ValidBoundedReceiver(int):
    def method[T: int](self: T) -> None: ...

class InvalidConstrainedReceiver:
    def method[T: (int, str)](self: T) -> None: ...

class ValidConstrainedReceiver(str):
    def method[T: (int, str)](self: T) -> None: ...

type ReceiverAlias[T] = T

class InvalidAliasedBoundedReceiver:
    def method[T: int](self: ReceiverAlias[T]) -> None: ...

class InvalidNestedBoundedReceiver(list[str]):
    def method[T: int](self: list[T]) -> None: ...

class InvalidUnionConstrainedReceiver:
    def method[T: (int, str)](self: T | None) -> None: ...

invalid_bound: Callable[[], None] = InvalidBoundedReceiver().method  # error: [invalid-assignment]
valid_bound: Callable[[], None] = ValidBoundedReceiver().method

invalid_constraints: Callable[[], None] = InvalidConstrainedReceiver().method  # error: [invalid-assignment]
valid_constraints: Callable[[], None] = ValidConstrainedReceiver().method

invalid_aliased_bound: Callable[[], None] = InvalidAliasedBoundedReceiver().method  # error: [invalid-assignment]

# TODO: Enforce valid specializations for TypeVars nested inside receiver annotations.
invalid_nested_bound: Callable[[], None] = InvalidNestedBoundedReceiver().method  # TODO: error: [invalid-assignment]
invalid_union_constraints: Callable[[], None] = InvalidUnionConstrainedReceiver().method  # TODO: error: [invalid-assignment]
```

When we coerce a generic callable into a `Callable` type, it remembers that it is generic:

```py
from ty_extensions._internal import into_regular_callable

# revealed: [T](t: T) -> T
reveal_type(into_regular_callable(identity))
# revealed: ty_extensions._internal.GenericContext[T@identity]
reveal_type(generic_context(into_regular_callable(identity)))
# revealed: Literal[1]
reveal_type(into_regular_callable(identity)(1))

# revealed: [**P, T](c: (**P) -> T) -> ((**P) -> T)
reveal_type(into_regular_callable(identity2))
# revealed: ty_extensions._internal.GenericContext[P@identity2, T@identity2]
reveal_type(generic_context(into_regular_callable(identity2)))
# revealed: [T](t: T) -> T
reveal_type(into_regular_callable(identity2)(identity))

# revealed: [T](t: T) -> C[T]
reveal_type(into_regular_callable(C))
# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(into_regular_callable(C)))
# revealed: C[int]
reveal_type(into_regular_callable(C)(1))
```

## Constructor callbacks with receiver-specific overloads

In the below example, the applicable `__new__` overload for `Factory[int]` returns a `Factory[int]`,
so construction also calls `__init__`. Callback compatibility therefore requires its `int` argument.
The overload that returns `str` applies only to `Factory[str]` and cannot bypass this requirement.

```py
from __future__ import annotations
from typing import Callable, overload

class Factory[T]:
    value: T

    @overload
    def __new__(cls: type[Factory[int]], *args: object) -> Factory[int]: ...
    @overload
    def __new__(cls: type[Factory[str]], *args: object) -> str: ...
    def __new__(cls, *args: object) -> Factory[int] | str:
        raise NotImplementedError

    def __init__(self, value: int) -> None: ...

valid: Callable[[int], Factory[int]] = Factory[int]
missing_argument: Callable[[], Factory[int]] = Factory[int]  # error: [invalid-assignment]
wrong_argument: Callable[[str], Factory[int]] = Factory[int]  # error: [invalid-assignment]
```

## Generic `__iter__` methods with explicit receivers

Binding `__iter__` to an `Unpacker[Iterable[int]]` infers `S` as `int` from the explicit
`self: Unpacker[Iterable[S]]` annotation. Calls to `list()` and `iter()` preserve this element type,
just as `tuple()` and `for` loops do.

Regression test for <https://github.com/astral-sh/ty/issues/3598>.

```py
from collections.abc import Iterable, Iterator

class Unpacker[T: Iterable[object]]:
    def __init__(self, it: T, /) -> None:
        self._it = it
    def __iter__[S](self: "Unpacker[Iterable[S]]") -> Iterator[S]:
        return iter(self._it)

def integers() -> Unpacker[Iterable[int]]:
    return Unpacker([1, 2, 3])

reveal_type(tuple(integers()))  # revealed: tuple[int, ...]
for x in integers():
    reveal_type(x)  # revealed: int
reveal_type(list(integers()))  # revealed: list[int]
reveal_type(iter(integers()))  # revealed: Iterator[int]
```

## Naming a generic `Callable`: type aliases

The easiest way to refer to a generic `Callable` type directly is via a type alias:

```py
from typing import Callable
from ty_extensions._internal import generic_context

type IdentityCallable[T] = Callable[[T], T]

def decorator_factory[T]() -> IdentityCallable[T]:
    def decorator[T](fn: T) -> T:
        return fn
    # revealed: ty_extensions._internal.GenericContext[T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

# revealed: [T'return](T'return, /) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions._internal.GenericContext[T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: Literal[1]
reveal_type(decorator_factory()(1))
```

## Naming a generic `Callable` with paramspecs: type aliases

The same pattern holds if the callable involves a paramspec.

```py
from typing import Callable
from ty_extensions._internal import generic_context

type IdentityCallable[**P, T] = Callable[[Callable[P, T]], Callable[P, T]]

def decorator_factory[**P, T]() -> IdentityCallable[P, T]:
    def decorator[**P, T](fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions._internal.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity[T](t: T) -> T:
    return t

# revealed: [**P'return, T'return]((**P'return) -> T'return, /) -> ((**P'return) -> T'return)
reveal_type(decorator_factory())
# revealed: ty_extensions._internal.GenericContext[P'return@decorator_factory, T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: [T](t: T) -> T
reveal_type(decorator_factory()(identity))
# revealed: Literal[1]
reveal_type(decorator_factory()(identity)(1))
```

## Naming a generic `Callable`: function return values

You can also return a generic `Callable` from a function. If a typevar _only_ appears inside of
`Callable`, and _only_ in return type position, then we treat the callable as generic, not the
function, just like above.

NOTE: This is one place where the PEP-695 syntax is misleading! It _looks_ like `decorator_factory`
is generic, since it contains a `[T]` binding context. However, we still notice that the only _use_
of `T` in the signature is in the return type, inside of a `Callable` — and so it is the returned
callable that is generic, not the function.

```py
from typing import Callable
from ty_extensions._internal import generic_context

def decorator_factory[T]() -> Callable[[T], T]:
    def decorator[T](fn: T) -> T:
        return fn
    # revealed: ty_extensions._internal.GenericContext[T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

# revealed: [T'return](T'return, /) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions._internal.GenericContext[T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: Literal[1]
reveal_type(decorator_factory()(1))
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable[T](t: T) -> Callable[[T], T]:
    raise NotImplementedError

# revealed: ty_extensions._internal.GenericContext[T@outside_callable]
reveal_type(generic_context(outside_callable))

# revealed: (int, /) -> int
reveal_type(outside_callable(1))
# revealed: None
reveal_type(generic_context(outside_callable(1)))
# error: [invalid-argument-type]
outside_callable(1)("string")
```

## Naming a generic `Callable` with paramspecs: function return values

The same pattern holds if the callable involves a paramspec.

```py
from typing import Callable
from ty_extensions._internal import generic_context

def decorator_factory[**P, T]() -> Callable[[Callable[P, T]], Callable[P, T]]:
    def decorator[**P, T](fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions._internal.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity[T](t: T) -> T:
    return t

# revealed: [**P'return, T'return]((**P'return) -> T'return, /) -> ((**P'return) -> T'return)
reveal_type(decorator_factory())
# revealed: ty_extensions._internal.GenericContext[P'return@decorator_factory, T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: [T](t: T) -> T
reveal_type(decorator_factory()(identity))
# revealed: Literal[1]
reveal_type(decorator_factory()(identity)(1))
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable[**P, T](func: Callable[P, T]) -> Callable[P, T]:
    raise NotImplementedError

# revealed: ty_extensions._internal.GenericContext[P@outside_callable, T@outside_callable]
reveal_type(generic_context(outside_callable))

def int_identity(x: int) -> int:
    return x

# revealed: (x: int) -> int
reveal_type(outside_callable(int_identity))
# revealed: None
reveal_type(generic_context(outside_callable(int_identity)))
# error: [invalid-argument-type]
outside_callable(int_identity)("string")
```

The function's type parameters are still in scope inside the body, even if they only appear in a
return-position `Callable` and are scoped to the returned callable:

```py
from typing import Callable, cast

def body_annotation[**P]() -> Callable[P, None]:
    local: Callable[P, None] = cast(Callable[P, None], object())
    return local
```

## Inferring an explicit `object` upper bound from a callable

A type variable in a callable parameter position is constrained from above because callable
parameters are contravariant. An explicit `object` upper bound is still inference evidence; it is
different from having no inferred bound at all.

```py
from typing import Callable

def infer_from_consumer[T](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_object(value: object) -> None: ...

reveal_type(infer_from_consumer(consume_object))  # revealed: object
```

## Intersecting inferred union upper bounds

Multiple callable arguments can infer multiple union upper bounds for the same type variable. We
keep those bounds factored and infer a compact type satisfying every bound rather than losing the
inference result while materializing their full cross product.

```py
from typing import Callable, final

def infer_from_consumers[T](
    left: Callable[[T], None],
    right: Callable[[T], None],
) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

@final
class E: ...

def consume_left(value: A | B | C) -> None: ...
def consume_right(value: B | D | E) -> None: ...

reveal_type(infer_from_consumers(consume_left, consume_right))  # revealed: B
```

## Union without intersection does not consider budget

If the precise inferred solution comes from a single union type, rather than an intersection of
several unions, we return the precise solution.

```py
from typing import Callable, final

def infer_from_consumer[T](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

@final
class E: ...

def consume(value: A | B | C | D | E) -> None: ...

reveal_type(infer_from_consumer(consume))  # revealed: A | B | C | D | E
```

The same union remains precise when it is defined through nested type aliases:

```py
type FirstTwo = A | B
type NextTwo = C | D
type Options = FirstTwo | NextTwo | E

def consume_alias(value: Options) -> None: ...

reveal_type(infer_from_consumer(consume_alias))  # revealed: A | B | C | D | E
```

## Overlapping inferred union upper bounds with few surviving alternatives

The individual union upper bounds can exceed the solution budget when only a few alternatives
survive their intersection. Disjoint alternatives do not count toward the budget.

```py
from typing import Callable, final

def infer_from_consumers[T](
    left: Callable[[T], None],
    right: Callable[[T], None],
) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

@final
class E: ...

@final
class F: ...

@final
class G: ...

@final
class H: ...

def consume_left(value: A | B | C | D | E) -> None: ...
def consume_right(value: A | B | F | G | H) -> None: ...

reveal_type(infer_from_consumers(consume_left, consume_right))  # revealed: A | B
```

Aliases for these unions also preserve the precise intersection in either argument order:

```py
type Left = A | B | C | D | E
type Right = A | B | F | G | H

def consume_left_alias(value: Left) -> None: ...
def consume_right_alias(value: Right) -> None: ...

reveal_type(infer_from_consumers(consume_left_alias, consume_right_alias))  # revealed: A | B
reveal_type(infer_from_consumers(consume_right_alias, consume_left_alias))  # revealed: A | B
```

## Intersecting aliased upper bounds exceeding the solution budget

Each consumer constrains `T` to a different union. The classes can share subclasses, so their
intersection has eight distinct alternatives. Type aliases do not exempt this expansion from the
solution budget: inference falls back to `Unknown` instead of constructing the entire intersection.

```py
from typing import Callable

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...

type First = A | B
type Second = C | D
type Third = E | F

def infer_from_consumers[T](
    first: Callable[[T], None],
    second: Callable[[T], None],
    third: Callable[[T], None],
) -> T:
    raise NotImplementedError

def consume_first(value: First) -> None: ...
def consume_second(value: Second) -> None: ...
def consume_third(value: Third) -> None: ...

reveal_type(infer_from_consumers(consume_first, consume_second, consume_third))  # revealed: Unknown
```

The same budget applies when an explicit union is intersected with aliased unions:

```py
def consume_explicit(value: E | F) -> None: ...

reveal_type(infer_from_consumers(consume_first, consume_second, consume_explicit))  # revealed: Unknown
```

## Narrowing negated intersection aliases

The negation of an aliased intersection acts as a union of negations. Intersecting three such upper
bounds can exceed the solution budget, but an additional literal upper bound leaves just one
solution. We infer the literal regardless of the consumer order.

```py
from typing import Callable, Literal
from ty_extensions import Intersection, Not

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...

type AB = Intersection[A, B]
type CD = Intersection[C, D]
type EF = Intersection[E, F]

def infer_from_consumers[T](
    first: Callable[[T], None],
    second: Callable[[T], None],
    third: Callable[[T], None],
    fourth: Callable[[T], None],
) -> T:
    raise NotImplementedError

def exclude_ab(value: Not[AB]) -> None: ...
def exclude_cd(value: Not[CD]) -> None: ...
def exclude_ef(value: Not[EF]) -> None: ...
def consume_literal(value: Literal[5]) -> None: ...

reveal_type(infer_from_consumers(exclude_ab, exclude_cd, exclude_ef, consume_literal))  # revealed: Literal[5]
reveal_type(infer_from_consumers(consume_literal, exclude_ab, exclude_cd, exclude_ef))  # revealed: Literal[5]
```

## Intersecting recursive inferred union upper bounds

A recursive alias can contribute an upper bound without expanding its nested occurrences. Here, only
`int` satisfies both consumers, regardless of their order.

```py
from typing import Callable

type Recursive = int | list[Recursive]

def infer_from_consumers[T](left: Callable[[T], None], right: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_recursive(value: Recursive) -> None: ...
def consume_int_or_str(value: int | str) -> None: ...

reveal_type(infer_from_consumers(consume_recursive, consume_int_or_str))  # revealed: int
reveal_type(infer_from_consumers(consume_int_or_str, consume_recursive))  # revealed: int
```

## Narrowing recursive inferred union upper bounds

The third consumer restricts two recursive unions to the literal `5`. Its position does not change
the inferred result.

```py
from typing import Callable, Literal

class A: ...
class B: ...
class C: ...
class D: ...

type First = Literal[5] | A | B | list[First]
type Second = Literal[5] | C | D | list[Second]

def infer_from_consumers[T](
    first: Callable[[T], None],
    second: Callable[[T], None],
    third: Callable[[T], None],
) -> T:
    raise NotImplementedError

def consume_first(value: First) -> None: ...
def consume_second(value: Second) -> None: ...
def consume_literal(value: Literal[5]) -> None: ...

reveal_type(infer_from_consumers(consume_first, consume_second, consume_literal))  # revealed: Literal[5]
reveal_type(infer_from_consumers(consume_literal, consume_first, consume_second))  # revealed: Literal[5]
```

## Contextual generic return exceeding the solution budget

A generic call can receive an upper bound from the type context in which its return value is used.
An existing union in that upper bound should not consume the bounded-intersection budget unless an
intersection actually needs to be distributed over it.

```py
from collections.abc import Sequence
from typing import Literal

def make_list[T](value: T) -> list[T]:
    return [value]

def consume(values: Sequence[Literal["a", "b", "c", "d", "e"]] | None) -> None: ...

consume(make_list("a"))
```

## Disjoint inferred union upper bounds

If `Never` is the only type satisfying all inferred union upper bounds, it is the valid inferred
specialization for the type variable.

```py
from typing import Callable, final

def infer_from_consumers[T](
    left: Callable[[T], None],
    right: Callable[[T], None],
) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

def consume_left(value: A | B) -> None: ...
def consume_right(value: C | D) -> None: ...

reveal_type(infer_from_consumers(consume_left, consume_right))  # revealed: Never
```

## Disjoint inferred union upper bounds exceeding the solution budget

Large disjoint union upper bounds also exceed the budget before we can discover that their precise
intersection is bottom.

```py
from typing import Callable, final

def infer_from_consumers[T](
    left: Callable[[T], None],
    right: Callable[[T], None],
) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

@final
class E: ...

@final
class F: ...

@final
class G: ...

@final
class H: ...

@final
class I: ...

@final
class J: ...

def consume_left(value: A | B | C | D | E) -> None: ...
def consume_right(value: F | G | H | I | J) -> None: ...

reveal_type(infer_from_consumers(consume_left, consume_right))  # revealed: Never
```

## Combining inferred and declared upper bounds

A declared type-variable bound also participates when selecting a type that satisfies an inferred
union upper bound.

```py
from typing import Any, Callable

def infer_str[T: str](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_int_or_str(value: int | str) -> None: ...

# revealed: str
reveal_type(infer_str(consume_int_or_str))
```

A gradual declared bound restricts which specializations are valid without becoming part of a
concrete specialization that already satisfies it:

```py
class GenericBase[T]: ...
class Child(GenericBase[int]): ...

def infer_child[T: GenericBase[Any]](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_child(value: Child) -> None: ...

# revealed: Child
reveal_type(infer_child(consume_child))
```

## Gradual class parameters

A callback that accepts `type[Any]` or `type[Unknown]` can accept any class object.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def invoke[T](callback: Callable[[type], T]) -> T:
    return callback(int)

def _(f: Callable[[type[Any]], int], g: Callable[[type[Unknown]], str]):
    reveal_type(invoke(f))  # revealed: int
    reveal_type(invoke(g))  # revealed: str
```

A gradual class argument can also satisfy a callback's metaclass parameter:

```py
class Meta(type): ...

def f(cls: Meta) -> int:
    return 1

def invoke_any[T](callback: Callable[[type[Any]], T], cls: type[Any]) -> T:
    return callback(cls)

def _(cls: type[Any]):
    reveal_type(invoke_any(f, cls))  # revealed: int
```

## Inferring gradual tuple returns with concrete bounds

A callback returning `tuple[Any, ...]` satisfies a fixed-length tuple bound because both its
elements and its length are gradual. Inference preserves the callback's return type.

```py
from typing import Any, Callable

def get_tuple() -> tuple[Any, ...]:
    return ()

def infer_fixed[T: tuple[int]](callback: Callable[[], T]) -> T:
    return callback()

reveal_type(infer_fixed(get_tuple))  # revealed: tuple[Any, ...]
```

The gradual length can also supply required elements at either end of a variable-length bound.

```py
def infer_prefix[T: tuple[int, *tuple[int, ...]]](callback: Callable[[], T]) -> T:
    return callback()

def infer_suffix[T: tuple[*tuple[int, ...], int]](callback: Callable[[], T]) -> T:
    return callback()

reveal_type(infer_prefix(get_tuple))  # revealed: tuple[Any, ...]
reveal_type(infer_suffix(get_tuple))  # revealed: tuple[Any, ...]
```

Fixed elements still have to satisfy the bound, and an ordinary homogeneous tuple does not have a
gradual length.

```py
def wrong_element() -> tuple[str, *tuple[Any, ...]]:
    return ("",)

def get_ints() -> tuple[int, ...]:
    return ()

infer_fixed(wrong_element)  # error: [invalid-argument-type]
infer_prefix(wrong_element)  # error: [invalid-argument-type]
infer_fixed(get_ints)  # error: [invalid-argument-type]
infer_prefix(get_ints)  # error: [invalid-argument-type]
infer_suffix(get_ints)  # error: [invalid-argument-type]
```

## Inferring type variables from gradual tuple elements

A callback returning a gradual-length tuple can constrain the type variables of a fixed-length
tuple.

```py
from typing import Any, Callable

def infer_pair[K, V](callback: Callable[[], tuple[K, V]]) -> tuple[K, V]:
    return callback()

def _(
    callback: Callable[[], tuple[Any, ...]],
    prefix: Callable[[], tuple[int, *tuple[Any, ...]]],
    suffix: Callable[[], tuple[*tuple[Any, ...], str]],
):
    reveal_type(infer_pair(callback))  # revealed: tuple[Any, Any]
    reveal_type(infer_pair(prefix))  # revealed: tuple[int, Any]
    reveal_type(infer_pair(suffix))  # revealed: tuple[Any, str]
```

A concrete homogeneous tuple does not guarantee the required length:

```py
def _(callback: Callable[[], tuple[int, ...]]):
    infer_pair(callback)  # error: [invalid-argument-type]
```

## Source type variables in gradual tuple returns

A callback's fixed tuple element can contain an outer type variable and still satisfy a concrete
bound. The gradual segment can be empty, and inference preserves the outer type variable.

```py
from typing import Any, Callable

def infer_tuple[R: tuple[object]](callback: Callable[[], R]) -> R:
    return callback()

def outer[T](callback: Callable[[], tuple[list[T], *tuple[Any, ...]]]) -> None:
    reveal_type(infer_tuple(callback))  # revealed: tuple[list[T@outer], *tuple[Any, ...]]
```

## Inferring `Never` from a callable parameter

`Never` is a valid upper-bound inference result and should not be replaced with the fallback for an
unsolved type variable.

```py
from typing import Callable, NoReturn

def infer_from_consumer[T](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_never(value: NoReturn) -> None: ...

reveal_type(infer_from_consumer(consume_never))  # revealed: Never
```

## Conflicting inferred lower and upper bounds

A concrete argument can infer a lower bound that is incompatible with an upper bound inferred from a
callable argument. Such a call is invalid rather than producing a solution outside the inferred
upper bound.

```py
from typing import Callable, final

def infer_with_consumer[T](value: T, consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

def consume_b(value: B) -> None: ...

infer_with_consumer(A(), consume_b)  # error: [invalid-argument-type]
```

A callable argument can make a call invalid even when another argument satisfies the declared
type-variable bound. The diagnostic should describe the incompatible callable rather than claim that
`Base` violates its own bound.

```py
from typing import Callable

class Base: ...
class Input: ...

def call[T: Base](callback: Callable[[T], T], value: T) -> None:
    raise NotImplementedError

def callback(value: Input) -> Base:
    raise NotImplementedError

# error: [invalid-argument-type] "Argument to function `call` is incorrect: Expected `(Base, /) -> Base`, found `def callback(value: Input) -> Base`"
call(callback, Base())
```

## Combined upper bounds uses redundancy

When solving an upper bound involving a union, we should use the same typing relation to look for
redundant elements as we use for unions in general.

```py
from typing import Any, Callable, final

def infer[T](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

@final
class A: ...

def callback(value: A | Any) -> None: ...

reveal_type(infer(callback))  # revealed: A | Any
```

## Overloaded callable as generic `Callable` argument

An overloaded callable should be assignable to a non-overloaded callable type when the overload set
as a whole is compatible with the target callable.

Each overload independently validates the same call, specializing `T` to `str` or `bytes`. Since the
function receives only a consumer of `T`, it has no way to produce a value of type `T` to return.
The return type must satisfy both specializations, so their intersection, `Never`, correctly
captures that no value can be returned.

```py
from typing import Callable, overload

def accepts_callable[T](converter: Callable[[T], None]) -> T:
    raise NotImplementedError

@overload
def overloaded_consumer(val: str) -> None: ...
@overload
def overloaded_consumer(val: bytes) -> None: ...
def overloaded_consumer(val: str | bytes) -> None:
    pass

def _() -> None:
    reveal_type(accepts_callable(overloaded_consumer))  # revealed: Never
```

An additional argument of type `T` supplies the return value and constrains the valid
specializations. A `str | bytes` value is accepted because the overload set covers both cases:

```py
def accepts_callable_and_value[T](converter: Callable[[T], None], value: T) -> T:
    converter(value)
    return value

def _(string: str, data: bytes, either: str | bytes) -> None:
    reveal_type(accepts_callable_and_value(overloaded_consumer, string))  # revealed: str
    reveal_type(accepts_callable_and_value(overloaded_consumer, data))  # revealed: bytes
    reveal_type(accepts_callable_and_value(overloaded_consumer, either))  # revealed: str | bytes
```

A `str | int` value is rejected because neither overload of the consumer accepts its `int`
alternative:

```py
def _(value: str | int) -> None:
    # TODO: Do not include the consumer's `bytes` alternative in the error-recovery return type.
    # error: [invalid-argument-type]
    reveal_type(accepts_callable_and_value(overloaded_consumer, value))  # revealed: str | bytes | int
```

Type variables inferred from the same overload remain correlated. Here, the valid assignments are
`T = int, U = str` and `T = str, U = int`; inference does not mix the input type from one overload
with the return type from the other:

```py
class Result[T]:
    def use(self, callback: Callable[[T], int]) -> int:
        raise NotImplementedError

def infer_pair[T: (int, str), U](converter: Callable[[T], U]) -> Result[tuple[T, U]]:
    raise NotImplementedError

@overload
def swap(value: int) -> str: ...
@overload
def swap(value: str) -> int: ...
def swap(value: int | str) -> int | str:
    raise NotImplementedError

def _() -> None:
    reveal_type(infer_pair(swap))  # revealed: Result[tuple[int, str]] & Result[tuple[str, int]]
```

When `T` is constrained to a union by other arguments, the overloaded callable must still be treated
as a whole to satisfy `Callable[[T], T]`.

```py
from typing import Callable, overload

def apply_twice[T](converter: Callable[[T], T], left: T, right: T) -> tuple[T, T]:
    return converter(left), converter(right)

@overload
def f(val: int) -> int: ...
@overload
def f(val: str) -> str: ...
def f(val: int | str) -> int | str:
    return val

x: int | str = 1
y: int | str = "a"

result = apply_twice(f, x, y)
# revealed: tuple[int | str, int | str]
reveal_type(result)
```

An overloaded callable returned from a generic callable factory should still be assignable to the
declared generic callable return type.

```py
from collections.abc import Callable, Coroutine
from typing import Any, overload

def singleton[S](flag: bool = False) -> Callable[[Callable[[int], S]], Callable[[int], S]]:
    @overload
    def wrapper[T](func: Callable[[int], Coroutine[Any, Any, T]]) -> Callable[[int], Coroutine[Any, Any, T]]: ...
    @overload
    def wrapper[U](func: Callable[[int], U]) -> Callable[[int], U]: ...
    def wrapper[T, U](func: Callable[[int], Coroutine[Any, Any, T] | U]) -> Callable[[int], Coroutine[Any, Any, T] | U]:
        return func

    return wrapper
```

## Dependent return types from generic callbacks

A generic identity callback can be used as either `Callable[[A], A]` or `Callable[[B], B]`: it
returns its argument unchanged:

```py
def identity[T](value: T) -> T:
    return value
```

This overloaded "consumer" can accept either an `A` or a `B`:

```py
from typing import Callable, overload

class A: ...
class B: ...

@overload
def consume(value: A) -> None: ...
@overload
def consume(value: B) -> None: ...
def consume(value: A | B) -> None: ...
```

If we pass both the callback and the consumer to a generic function, we can solve `T` (and thus also
`R`) to either `A` or `B`. This should allow us to infer `A & B` as the return type.

```py
def infer_result[T, R](callback: Callable[[T], R], consumer: Callable[[T], None]) -> R:
    raise NotImplementedError

# TODO: revealed: A & B
reveal_type(infer_result(identity, consume))  # revealed: A | B
```

Before intersecting the results, we check that each specialization accepts the original callbacks.
This subtype check does not yet infer `identity`'s own type variable when comparing it with
`Callable[[A], A]` or `Callable[[B], B]`. Neither specialization currently passes this check, so we
infer `A | B` here until this limitation is fixed.

If we additionally supply a value, that selects the specific consumer overload that accepts it. The
identity callback's result type follows that selected argument type:

```py
def infer_result_with_value[T, R](callback: Callable[[T], R], consumer: Callable[[T], None], value: T) -> R:
    return callback(value)

def _(a: A, b: B) -> None:
    reveal_type(infer_result_with_value(identity, consume, a))  # revealed: A
    reveal_type(infer_result_with_value(identity, consume, b))  # revealed: B
```

## Inferred type-guard return alternatives

Type-guard functions return booleans. The types inside `TypeGuard` and `TypeIs` describe how their
arguments can be narrowed, not the values they return. A callback can have two type-guard signatures
and still return normally. Intersecting those return annotations as ordinary types can instead
produce `Never`, incorrectly suggesting that the call cannot return.

A generic function that calls the callback and returns its result has the same behavior, even if its
own return annotation is only a type variable. Its inferred return type therefore retains the union
of the guard annotations rather than becoming `Never`.

```py
from collections.abc import Callable
from typing import TypeGuard
from typing_extensions import TypeIs
from ty_extensions import Intersection

class A: ...
class B: ...

def invoke[R](callback: Callable[[object], R], value: object) -> R:
    return callback(value)

def _(callback: Intersection[Callable[[object], TypeGuard[A]], Callable[[object], TypeGuard[B]]]) -> None:
    reveal_type(invoke(callback, object()))  # revealed: TypeGuard[A] | TypeGuard[B]

def _(callback: Intersection[Callable[[object], TypeIs[A]], Callable[[object], TypeIs[B]]]) -> None:
    reveal_type(invoke(callback, object()))  # revealed: TypeIs[A] | TypeIs[B]
```

## Inferred mutable return alternatives

Multiple return types can be intersected when they describe properties that hold simultaneously for
the same returned value. A fresh list (or other invariant container) introduces a different
situation: the same expression, `[]`, can be correctly typed as either `list[A]` or `list[B]`,
depending on the expected type. These successful typings make different (and mutually exclusive)
choices for the newly constructed list's static element type. They do not give one returned list
both specializations at once.

The two signatures therefore provide alternative valid typings for each call, not two simultaneous
properties of a single result. Their return-type intersection is `Never`, since these incompatible
invariant list specializations are disjoint, but that does not mean the callback cannot return. We
conservatively keep `list[A] | list[B]` instead for now. TODO this may be overly conservative; in
the absence of type context it would be less restrictive to just pick one type or the other (since
either is a valid inference), though it's hard to find a compelling rationale for which to pick.

```py
from collections.abc import Callable
from ty_extensions import Intersection

class A: ...
class B: ...

def invoke[R](callback: Callable[[object], R], value: object) -> R:
    return callback(value)

def _(callback: Intersection[Callable[[object], list[A]], Callable[[object], list[B]]]) -> None:
    reveal_type(invoke(callback, object()))  # revealed: list[A] | list[B]
```

An assignment to a variable annotated as `list[A]` or `list[B]` should select the corresponding
callback signature. Each signature independently accepts the call and returns the required type:

```py
def _(callback: Intersection[Callable[[object], list[A]], Callable[[object], list[B]]]) -> None:
    # TODO: This assignment should succeed with `R = list[A]`.
    # error: [invalid-assignment] "Object of type `list[A] | list[B]` is not assignable to `list[A]`"
    a: list[A] = invoke(callback, object())
    # TODO: This assignment should succeed with `R = list[B]`.
    # error: [invalid-assignment] "Object of type `list[A] | list[B]` is not assignable to `list[B]`"
    b: list[B] = invoke(callback, object())
```

A fixed mutable component does not prevent other components of the result from being refined. Both
callback signatures below return the same `list[int]` type in the second tuple position, so their
return types can be intersected. Either tuple type can also serve as the expected return type:

```py
def _(
    callback: Intersection[
        Callable[[object], tuple[A, list[int]]],
        Callable[[object], tuple[B, list[int]]],
    ],
) -> None:
    reveal_type(invoke(callback, object()))  # revealed: tuple[A, list[int]] & tuple[B, list[int]]
    a: tuple[A, list[int]] = invoke(callback, object())
    b: tuple[B, list[int]] = invoke(callback, object())
```

An alias around the tuple does not change which component is invariant or prevent the refinement:

```py
type Alias[T] = tuple[T, list[int]]

def _(callback: Intersection[Callable[[object], Alias[A]], Callable[[object], Alias[B]]]) -> None:
    reveal_type(invoke(callback, object()))  # revealed: tuple[A, list[int]] & tuple[B, list[int]]
```

The invariant components must agree at each position. Swapping `list[A]` and `list[B]` does not make
the two tuple types compatible, so inference retains their union:

```py
def _(
    callback: Intersection[
        Callable[[object], tuple[list[A], list[B]]],
        Callable[[object], tuple[list[B], list[A]]],
    ],
) -> None:
    reveal_type(invoke(callback, object()))  # revealed: tuple[list[A], list[B]] | tuple[list[B], list[A]]
```

Union alternatives can associate each mutable component with a different type in another position.
These associations differ between the signatures, so inference keeps the full union:

```py
def _(
    callback: Intersection[
        Callable[[object], tuple[list[int], int] | tuple[list[str], str]],
        Callable[[object], tuple[list[int], str] | tuple[list[str], int]],
    ],
) -> None:
    # revealed: tuple[list[int], int] | tuple[list[str], str] | tuple[list[int], str] | tuple[list[str], int]
    reveal_type(invoke(callback, object()))
```

A broader union return does not make incompatible mutable returns safe to intersect:

```py
class C: ...
class All(A, B, C): ...

def invoke_all[T, R](callback: Callable[[T], R], value: T) -> R:
    return callback(value)

def _(
    callback: Intersection[
        Callable[[A], list[A] | list[B]],
        Callable[[B], list[A]],
        Callable[[C], list[B]],
    ],
) -> None:
    reveal_type(invoke_all(callback, All()))  # revealed: list[A] | list[B]
```

For now, wrapping a mutable component in a union can prevent refinement even when other alternatives
agree on that component. This conservative fallback does not depend on the callback signature order:

```py
def _(
    first: Intersection[
        Callable[[A], tuple[A, list[int]]],
        Callable[[B], tuple[B, list[int]]],
        Callable[[C], tuple[A, list[int]] | int],
    ],
    reordered: Intersection[
        Callable[[B], tuple[B, list[int]]],
        Callable[[A], tuple[A, list[int]]],
        Callable[[C], tuple[A, list[int]] | int],
    ],
) -> None:
    # TODO: revealed: tuple[A, list[int]] & tuple[B, list[int]]
    reveal_type(invoke_all(first, All()))  # revealed: tuple[A, list[int]] | int | tuple[B, list[int]]
    # TODO: revealed: tuple[B, list[int]] & tuple[A, list[int]]
    reveal_type(invoke_all(reordered, All()))  # revealed: tuple[B, list[int]] | tuple[A, list[int]] | int
```

The same distinction applies to generic classes with both covariant and invariant parameters.
`Wrapper` only produces `T`, while its writable `value` attribute makes `U` invariant:

```py
class Wrapper[T, U]:
    value: U

    def get(self) -> T:
        raise NotImplementedError
```

When the signatures agree on `U = int`, their return types can be intersected. If `U` varies between
`int` and `str`, inference retains the union:

```py
def _(
    fixed: Intersection[Callable[[object], Wrapper[A, int]], Callable[[object], Wrapper[B, int]]],
    varying: Intersection[Callable[[object], Wrapper[A, int]], Callable[[object], Wrapper[B, str]]],
) -> None:
    reveal_type(invoke(fixed, object()))  # revealed: Wrapper[A, int] & Wrapper[B, int]
    reveal_type(invoke(varying, object()))  # revealed: Wrapper[A, int] | Wrapper[B, str]
```

A tuple subclass can have invariant attributes beyond its tuple elements. Identical inherited tuple
elements do not make different specializations of its writable `value` attribute compatible:

```py
class TupleWrapper[T](tuple[int, list[int]]):
    value: T

def _(
    callback: Intersection[Callable[[object], TupleWrapper[A]], Callable[[object], TupleWrapper[B]]],
) -> None:
    reveal_type(invoke(callback, object()))  # revealed: TupleWrapper[A] | TupleWrapper[B]
```

## Inferred TypedDict return alternatives

Writable `TypedDict` fields are invariant even when the dictionaries have no type parameters. A
fresh `{}` can be typed as either dictionary below because their fields are optional. Inference
retains the union of these return types rather than their disjoint intersection, `Never`:

```py
from typing import Callable, TypedDict
from ty_extensions import Intersection

class IntDict(TypedDict, total=False):
    value: int

class StrDict(TypedDict, total=False):
    value: str

def invoke[R](callback: Callable[[], R]) -> R:
    return callback()

def _(callback: Intersection[Callable[[], IntDict], Callable[[], StrDict]]) -> None:
    reveal_type(invoke(callback))  # revealed: IntDict | StrDict
```

The same field differences matter when the dictionaries are nested in a tuple:

```py
def _(callback: Intersection[Callable[[], tuple[IntDict]], Callable[[], tuple[StrDict]]]) -> None:
    reveal_type(invoke(callback))  # revealed: tuple[IntDict] | tuple[StrDict]
```

A `ReadOnly` field can still contain a mutable value. A fresh empty list can have either element
type, so these dictionary returns also remain a union:

```py
from typing_extensions import ReadOnly

class IntListDict(TypedDict):
    value: ReadOnly[list[int]]

class StrListDict(TypedDict):
    value: ReadOnly[list[str]]

def _(callback: Intersection[Callable[[], IntListDict], Callable[[], StrListDict]]) -> None:
    reveal_type(invoke(callback))  # revealed: IntListDict | StrListDict
```

An unchanged dictionary component still allows the other tuple component to be refined:

```py
class A: ...
class B: ...

def _(
    callback: Intersection[Callable[[], tuple[A, IntDict]], Callable[[], tuple[B, IntDict]]],
) -> None:
    reveal_type(invoke(callback))  # revealed: tuple[A, IntDict] & tuple[B, IntDict]
```

For now, the conservative fallback also applies to distinct fully read-only dictionaries. These
fields have no invariant components, so their return types could instead be intersected:

```py
class AView(TypedDict):
    value: ReadOnly[A]

class BView(TypedDict):
    value: ReadOnly[B]

def _(callback: Intersection[Callable[[], AView], Callable[[], BView]]) -> None:
    # TODO: revealed: AView & BView
    reveal_type(invoke(callback))  # revealed: AView | BView
```

## Inferred protocol return alternatives

A fresh empty list can have either element type, but cannot safely expose both mutable views at
once. Even without type parameters, protocol returns can contain incompatible invariant members:

```py
from typing import Callable, Protocol, overload

class A: ...
class B: ...
class Both(A, B): ...

class IntList(Protocol):
    value: list[int]

class StrList(Protocol):
    value: list[str]

class Box[T]:
    value: list[T]

    def __init__(self) -> None:
        self.value = []

@overload
def empty(value: A) -> IntList: ...
@overload
def empty(value: B) -> StrList: ...
def empty(value: A | B) -> IntList | StrList:
    return Box()

def invoke[T, R](callback: Callable[[T], R], value: T) -> R:
    return callback(value)

def _() -> None:
    reveal_type(invoke(empty, Both()))  # revealed: IntList | StrList
```

Making the property read-only does not make its list immutable. These returns also remain a union:

```py
class IntListView(Protocol):
    @property
    def value(self) -> list[int]: ...

class StrListView(Protocol):
    @property
    def value(self) -> list[str]: ...

@overload
def readonly_empty(value: A) -> IntListView: ...
@overload
def readonly_empty(value: B) -> StrListView: ...
def readonly_empty(value: A | B) -> IntListView | StrListView:
    return Box()

def _() -> None:
    reveal_type(invoke(readonly_empty, Both()))  # revealed: IntListView | StrListView
```

An unchanged protocol component still allows the other tuple component to be refined:

```py
@overload
def with_fixed(value: A) -> tuple[A, IntList]: ...
@overload
def with_fixed(value: B) -> tuple[B, IntList]: ...
def with_fixed(value: A | B) -> tuple[A | B, IntList]:
    return value, Box[int]()

def _() -> None:
    reveal_type(invoke(with_fixed, Both()))  # revealed: tuple[A, IntList] & tuple[B, IntList]
```

Specializations of the same generic protocol can also be compared without inspecting its members.
Here, only the covariant parameter varies; the writable `state` has the same type in both returns:

```py
class View[T](Protocol):
    state: list[int]

    @property
    def value(self) -> T: ...

@overload
def view(value: A) -> View[A]: ...
@overload
def view(value: B) -> View[B]: ...
def view(value: A | B) -> View[A] | View[B]:
    raise NotImplementedError

def _() -> None:
    reveal_type(invoke(view, Both()))  # revealed: View[A] & View[B]
```

## Multiple occurrences of a higher-order generic callable

If a generic callable is used more than once in a higher-order call, each occurrence should get its
own fresh typevars. In this example, the outer `partial` call receives a second, independent
occurrence of `partial` as its first argument, and `drop` as its second argument.

```py
from typing import Callable

def partial[A, B, C](c: Callable[[A, B], C], a: A) -> Callable[[B], C]:
    def inner(b: B) -> C:
        return c(a, b)
    return inner

def drop[X, Y](x: X, y: Y) -> Y:
    return y

# TODO: revealed: Literal["x"]
# We are correctly combining the constraint sets from both arguments of the outer
# `partial(partial, drop)` call: one from passing `partial` as `c`, and one from passing `drop` as
# `a`. However, we do that after having existentially quantified away the typevars from the generic
# `partial` when it's used as an argument, so this remains `Unknown` even after generic callable
# occurrences are freshened.
reveal_type(partial(partial, drop)(1)("x"))  # revealed: Unknown
# TODO: revealed: Literal[1]
reveal_type(partial(partial, drop)("x")(1))  # revealed: Unknown
```

## SymPy one-import MRE scaffold (multi-file)

Reduced regression lock for a SymPy overload/protocol shape that can panic in the
overload-assignability path.

```py
from __future__ import annotations

from sympy.polys.compatibility import Domain, IPolys
from typing import overload

class DefaultPrinting:
    pass

class PolyRing[T](DefaultPrinting, IPolys[T]):
    symbols: tuple[object, ...]
    domain: Domain[T]

    def clone(
        self,
        symbols: object | None = None,
        domain: object | None = None,
        order: object | None = None,
    ) -> PolyRing[T]:
        return self

    @overload
    def __getitem__(self, key: int) -> PolyRing[T]: ...
    @overload
    def __getitem__(self, key: slice) -> PolyRing[T] | Domain[T]: ...
    def __getitem__(self, key: slice | int) -> PolyRing[T] | Domain[T]:
        symbols = self.symbols[key]
        if not symbols:
            return self.domain
        return self.clone(symbols=symbols)

def takes_ring(x: PolyRing[int]) -> None:
    reveal_type(x[0])  # revealed: PolyRing[int]
    reveal_type(x[:])  # revealed: PolyRing[int] | Domain[int]
```

`sympy/polys/compatibility.pyi`:

```pyi
from __future__ import annotations

from typing import Protocol, overload

class Domain[T]: ...

class IPolys[T](Protocol):
    @overload
    def clone(
        self,
        symbols: object | None = None,
        domain: None = None,
        order: None = None,
    ) -> IPolys[T]: ...
    @overload
    def clone[S](
        self,
        symbols: object | None = None,
        *,
        domain: Domain[S],
        order: None = None,
    ) -> IPolys[S]: ...
    @overload
    def __getitem__(self, key: int) -> IPolys[T]: ...
    @overload
    def __getitem__(self, key: slice) -> IPolys[T] | Domain[T]: ...
```

## Returned callables with recursive parameter aliases

A type variable used by a recursive parameter alias belongs to the function. The returned callable
uses the type argument inferred from that parameter.

```py
from typing import Callable

type Tree[T] = tuple[T, Tree[T] | None]

def make[T](value: Tree[T]) -> Callable[[T], T]:
    raise NotImplementedError

callback = make((1, None))
reveal_type(callback)  # revealed: (int, /) -> int
callback("bad")  # error: [invalid-argument-type]
```

The type argument can also change at each recursive step.

```py
type Growing[T] = tuple[T, Growing[list[T]] | None]

def make_growing[T](value: Growing[T]) -> Callable[[T], T]:
    raise NotImplementedError

callback_growing = make_growing((1, None))
reveal_type(callback_growing)  # revealed: (int, /) -> int
callback_growing("bad")  # error: [invalid-argument-type]
```
