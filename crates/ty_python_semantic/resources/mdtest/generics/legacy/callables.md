# Generic callables: Legacy syntax

## Callables can be generic

Many items that are callable can also be generic. Generic functions are the most obvious example:

```py
from typing import Callable, ParamSpec, TypeVar
from ty_extensions._internal import generic_context

P = ParamSpec("P")
T = TypeVar("T")

def identity(t: T) -> T:
    return t

# revealed: ty_extensions._internal.GenericContext[T@identity]
reveal_type(generic_context(identity))
# revealed: Literal[1]
reveal_type(identity(1))

def identity2(c: Callable[P, T]) -> Callable[P, T]:
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
from typing import Generic

class C(Generic[T]):
    def __init__(self, t: T) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: C[int]
reveal_type(C(1))
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

## Naming a generic `Callable`: type aliases

The easiest way to refer to a generic `Callable` type directly is via a type alias:

```py
from typing import Callable, TypeVar
from ty_extensions._internal import generic_context

T = TypeVar("T")

IdentityCallable = Callable[[T], T]

def decorator_factory() -> IdentityCallable[T]:
    def decorator(fn: T) -> T:
        return fn
    # revealed: ty_extensions._internal.GenericContext[T@decorator]
    reveal_type(generic_context(decorator))
    # revealed: Literal[1]
    reveal_type(decorator(1))

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
from typing import Callable, ParamSpec, TypeVar
from ty_extensions._internal import generic_context

P = ParamSpec("P")
T = TypeVar("T")

IdentityCallable = Callable[[Callable[P, T]], Callable[P, T]]

def decorator_factory() -> IdentityCallable[P, T]:
    def decorator(fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions._internal.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity(t: T) -> T:
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

```py
from typing import Callable, TypeVar
from ty_extensions._internal import generic_context

T = TypeVar("T")

def decorator_factory() -> Callable[[T], T]:
    def decorator(fn: T) -> T:
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
def outside_callable(t: T) -> Callable[[T], T]:
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
from typing import Callable, ParamSpec, TypeVar
from ty_extensions._internal import generic_context

P = ParamSpec("P")
T = TypeVar("T")

def decorator_factory() -> Callable[[Callable[P, T]], Callable[P, T]]:
    def decorator(fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions._internal.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity(t: T) -> T:
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

A legacy factory's return statements are checked against the lexical form of its return type. This
also applies when the returned callable accepts and returns another callable:

```py
from typing import NoReturn

class WrappedCallable:
    def __call__(self, *args: object, **kwargs: object) -> NoReturn:
        raise NotImplementedError

def nested_callable_factory() -> Callable[[Callable[P, T]], Callable[P, T]]:
    return lambda callback: WrappedCallable()
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable(func: Callable[P, T]) -> Callable[P, T]:
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

## Union without intersection does not consider budget

A single union upper bound remains precise even when it has more alternatives than the solution
budget. Nested `TypeAliasType` aliases preserve the same result.

```py
from typing import Callable, TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...

def infer_from_consumer(consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume(value: A | B | C | D | E) -> None: ...

reveal_type(infer_from_consumer(consume))  # revealed: A | B | C | D | E
```

The aliases retain nested union members until inference expands them:

```py
FirstTwo = TypeAliasType("FirstTwo", A | B)
NextTwo = TypeAliasType("NextTwo", C | D)
Options = TypeAliasType("Options", FirstTwo | NextTwo | E)

def consume_alias(value: Options) -> None: ...

reveal_type(infer_from_consumer(consume_alias))  # revealed: A | B | C | D | E
```

## Overlapping inferred union upper bounds with few surviving alternatives

The individual union upper bounds can exceed the solution budget when only a few alternatives
survive their intersection. Disjoint alternatives do not count toward the budget.

```py
from typing import Callable, TypeVar, final
from typing_extensions import TypeAliasType

T = TypeVar("T")

def infer_from_consumers(left: Callable[[T], None], right: Callable[[T], None]) -> T:
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
Left = TypeAliasType("Left", A | B | C | D | E)
Right = TypeAliasType("Right", A | B | F | G | H)

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
from typing import Callable, TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...

First = TypeAliasType("First", A | B)
Second = TypeAliasType("Second", C | D)
Third = TypeAliasType("Third", E | F)

def infer_from_consumers(
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
from typing import Callable, Literal, TypeVar
from typing_extensions import TypeAliasType
from ty_extensions import Intersection, Not

T = TypeVar("T")

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...

AB = TypeAliasType("AB", Intersection[A, B])
CD = TypeAliasType("CD", Intersection[C, D])
EF = TypeAliasType("EF", Intersection[E, F])

def infer_from_consumers(
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
from typing import Callable, TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")
Recursive = TypeAliasType("Recursive", "int | list[Recursive]")

def infer_from_consumers(left: Callable[[T], None], right: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_recursive(value: Recursive) -> None: ...
def consume_int_or_str(value: int | str) -> None: ...

reveal_type(infer_from_consumers(consume_recursive, consume_int_or_str))  # revealed: int
reveal_type(infer_from_consumers(consume_int_or_str, consume_recursive))  # revealed: int
```

## Overloaded callable as generic `Callable` argument

An overloaded callable should be assignable to a non-overloaded callable type when the overload set
as a whole is compatible with the target callable.

Each overload independently validates the same call, specializing `T` to `str` or `bytes`. Since the
function receives only a consumer of `T`, it has no way to produce a value of type `T` to return.
The return type must satisfy both specializations, so their intersection, `Never`, correctly
captures that no value can be returned.

```py
from typing import Callable, TypeVar, overload

T = TypeVar("T")

def accepts_callable(converter: Callable[[T], None]) -> T:
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
def accepts_callable_and_value(converter: Callable[[T], None], value: T) -> T:
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

When overloads exchange their input and output types, inference preserves each input-output pair.
The constrained input type excludes `Never`, so each specialization selects one of the overloads. A
covariant wrapper keeps the pairs visible in the return type instead of collapsing their
intersection to `Never`:

```py
from typing import Generic

ResultT = TypeVar("ResultT", covariant=True)
PairT = TypeVar("PairT", int, str)
U = TypeVar("U")

class Result(Generic[ResultT]):
    def use(self, callback: Callable[[ResultT], int]) -> int:
        raise NotImplementedError

def infer_pair(converter: Callable[[PairT], U]) -> Result[tuple[PairT, U]]:
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

## Rejected overloaded callbacks preserve valid specializations

An overloaded callback may contain one alternative whose return type violates a type variable's
upper bound or declared constraints. The valid alternative must determine the specialization
regardless of the order in which the overloads appear.

```py
from typing import Callable, TypeVar, overload

Bounded = TypeVar("Bounded", bound=int)
Constrained = TypeVar("Constrained", int, bytes)

@overload
def invalid_first(value: str) -> str: ...
@overload
def invalid_first(value: int) -> int: ...
def invalid_first(value: str | int) -> str | int:
    return value

@overload
def invalid_last(value: int) -> int: ...
@overload
def invalid_last(value: str) -> str: ...
def invalid_last(value: str | int) -> str | int:
    return value

def infer_bound(callback: Callable[..., Bounded]) -> Bounded:
    raise NotImplementedError

def infer_constrained(callback: Callable[..., Constrained]) -> Constrained:
    raise NotImplementedError

reveal_type(infer_bound(invalid_first))  # revealed: int
reveal_type(infer_bound(invalid_last))  # revealed: int
reveal_type(infer_constrained(invalid_first))  # revealed: int
reveal_type(infer_constrained(invalid_last))  # revealed: int
```

## Overloaded methods with `Self` passed to a decorator

A concrete overload can be fully solved while another valid overload keeps its receiver and return
type correlated through `Self`. Ideally, the generic alternative would pass through the solver with
that correlation preserved; this is not yet supported:

```py
from typing import Callable, TypeVar, overload
from typing_extensions import Self

A = TypeVar("A")
B = TypeVar("B")
R = TypeVar("R")

def identity(fn: Callable[[A, B], R]) -> Callable[[A, B], R]:
    return fn

class Expr: ...

class Matrix:
    @overload
    def __mul__(self, other: "Matrix") -> "Matrix": ...
    @overload
    def __mul__(self, other: Expr) -> Self: ...
    def __mul__(self, other: "Matrix | Expr") -> "Matrix | Self":
        raise NotImplementedError

class SpecialMatrix(Matrix): ...

matrix = Matrix()
special = SpecialMatrix()
expr = Expr()

# TODO: Preserve both overloads, including the generic `Self` alternative, without erroring.
# error: [invalid-argument-type]
mul = identity(Matrix.__mul__)
reveal_type(mul)  # revealed: (Matrix, Matrix | Expr, /) -> Matrix
reveal_type(mul(matrix, expr))  # revealed: Matrix
reveal_type(mul(matrix, matrix))  # revealed: Matrix
# TODO: revealed: SpecialMatrix
reveal_type(mul(special, expr))  # revealed: Matrix
reveal_type(mul(special, special))  # revealed: Matrix
```

## Overloaded callable with a constrained type variable

When `T` is constrained to a union by other arguments, the overloaded callable must still be treated
as a whole to satisfy `Callable[[T], T]`.

```py
from typing import Callable, TypeVar, overload

T = TypeVar("T")

def apply_twice(converter: Callable[[T], T], left: T, right: T) -> tuple[T, T]:
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

## Overloaded callable returned by a generic factory

An overloaded callable returned from a generic callable factory should still be assignable to the
declared generic callable return type.

```py
from collections.abc import Callable, Coroutine
from typing import Any, TypeVar, overload

S = TypeVar("S")
T = TypeVar("T")
U = TypeVar("U")

def singleton(flag: bool = False) -> Callable[[Callable[[int], S]], Callable[[int], S]]:
    @overload
    def wrapper(func: Callable[[int], Coroutine[Any, Any, T]]) -> Callable[[int], Coroutine[Any, Any, T]]: ...
    @overload
    def wrapper(func: Callable[[int], U]) -> Callable[[int], U]: ...
    def wrapper(func: Callable[[int], Coroutine[Any, Any, T] | U]) -> Callable[[int], Coroutine[Any, Any, T] | U]:
        return func

    return wrapper
```

## Dependent return types from generic callbacks

A generic identity callback returns the type it receives. Each overload of the consumer selects a
different argument type for that callback, which also determines its result type. The enclosing
call's return type should satisfy both specializations, giving `A & B`. We currently retain `A | B`
because subtype revalidation does not recognize the generic callback as a subtype of either
specialized callable type:

```py
from typing import Callable, TypeVar, overload

T = TypeVar("T")
R = TypeVar("R")

class A: ...
class B: ...

def infer_result(callback: Callable[[T], R], consumer: Callable[[T], None]) -> R:
    raise NotImplementedError

def identity(value: T) -> T:
    return value

@overload
def consume(value: A) -> None: ...
@overload
def consume(value: B) -> None: ...
def consume(value: A | B) -> None: ...

# TODO: revealed: A & B
reveal_type(infer_result(identity, consume))  # revealed: A | B
```

Supplying a value selects the consumer overload that accepts it. The identity callback's result type
follows that selected argument type:

```py
def infer_result_with_value(callback: Callable[[T], R], consumer: Callable[[T], None], value: T) -> R:
    return callback(value)

def _(a: A, b: B) -> None:
    reveal_type(infer_result_with_value(identity, consume, a))  # revealed: A
    reveal_type(infer_result_with_value(identity, consume, b))  # revealed: B
```

## Return type inference from partially annotated overloads

The catch-all overload returns `object`, which is preserved when inferring a return type from the
whole callback even though the literal-specific overloads have unannotated return types.

```py
from typing import Callable, Literal, TypeVar, overload
from typing_extensions import assert_type

R = TypeVar("R")
T = TypeVar("T")

def infer_return(callback: Callable[[T], R]) -> R:
    raise NotImplementedError

@overload
def callback(value: Literal["a"]): ...
@overload
def callback(value: Literal["b"]): ...
@overload
def callback(value: Literal["c"]): ...
@overload
def callback(value: Literal["d", "e"]): ...
@overload
def callback(value: Literal["f", "g"]): ...
@overload
def callback(value: Literal["h", "i"]): ...
@overload
def callback(value: Literal["j", "k"]): ...
@overload
def callback(value: object) -> object: ...
def callback(value):
    raise NotImplementedError

assert_type(infer_return(callback), object)
```

## Generic inference after projection budget exhaustion

Each tuple element independently matches one of the callback's overloads. The combined alternative
bindings exceed generic inference's projection limits. The precise type of `default=0` does not
replace the missing callback evidence: we recover with `Unknown` in either argument order.

```py
from typing import Callable, Literal, TypeVar, overload
from typing_extensions import assert_type
from ty_extensions._internal import Unknown

R = TypeVar("R")
T = TypeVar("T")
U = TypeVar("U")
V = TypeVar("V")

def infer_return(callback: tuple[Callable[[T], R], Callable[[U], R], Callable[[V], R]], default: R) -> R:
    raise NotImplementedError

@overload
def callback(value: Literal[0, 1]): ...
@overload
def callback(value: Literal[2, 3]): ...
@overload
def callback(value: Literal[4, 5]): ...
@overload
def callback(value: Literal[6, 7]): ...
@overload
def callback(value: Literal[8, 9]): ...
@overload
def callback(value: Literal[10, 11]): ...
@overload
def callback(value: Literal[12, 13]): ...
@overload
def callback(value: Literal[14, 15]): ...
@overload
def callback(value: Literal[16, 17]): ...
@overload
def callback(value: Literal[18, 19]): ...
@overload
def callback(value: object) -> object: ...
def callback(value):
    raise NotImplementedError

assert_type(infer_return((callback, callback, callback), 0), Unknown)
assert_type(infer_return(default=0, callback=(callback, callback, callback)), Unknown)
```

## Contextual preference after solution budget exhaustion

The return context prefers `object` for the list's element type. It also requires the repeated
payload type to satisfy both `A | B` and `C | D | E`, whose intersection exceeds the solution
budget. Argument inference supplies the payload type and two source element alternatives, but does
not make the contextual inference complete. The result keeps the merged source element type rather
than intersecting the alternative tuple returns.

```py
from typing import Generic, TypeVar
from ty_extensions import Intersection

T = TypeVar("T")
U = TypeVar("U")
V = TypeVar("V")
Element = TypeVar("Element", covariant=True)

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...

class Source(Generic[Element]):
    def get(self) -> Element:
        raise NotImplementedError

def make(value: T, payload: U, source: Source[V]) -> tuple[list[T], U, U, V]:
    raise NotImplementedError

def _(payload: Intersection[A, C], source: Intersection[Source[D], Source[E]]) -> None:
    # revealed: tuple[list[object], A & C, A & C, D | E]
    result: tuple[list[object], A | B, C | D | E, object] = reveal_type(make(1, payload, source))
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
from typing import TypeGuard, TypeVar
from typing_extensions import TypeIs
from ty_extensions import Intersection

R = TypeVar("R")

class A: ...
class B: ...

def invoke(callback: Callable[[object], R], value: object) -> R:
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
from typing import TypeVar
from ty_extensions import Intersection

R = TypeVar("R")

class A: ...
class B: ...

def invoke(callback: Callable[[object], R], value: object) -> R:
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

## Multiple occurrences of a higher-order generic callable

If a generic callable is used more than once in a higher-order call, each occurrence should get its
own fresh typevars. In this example, the outer `partial` call receives a second, independent
occurrence of `partial` as its first argument, and `drop` as its second argument.

```py
from typing import Callable, TypeVar

A = TypeVar("A")
B = TypeVar("B")
C = TypeVar("C")
X = TypeVar("X")
Y = TypeVar("Y")

def partial(c: Callable[[A, B], C], a: A) -> Callable[[B], C]:
    def inner(b: B) -> C:
        return c(a, b)
    return inner

def drop(x: X, y: Y) -> Y:
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

## ParamSpec substitution preserves non-gradual variadic parameters

Specializing variadic parameter types to `Any` does not make the parameter list gradual when it is
substituted for a `ParamSpec`:

```py
from typing import Any, Callable, Generic, ParamSpec, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of

P = ParamSpec("P")
T = TypeVar("T")

class C(Generic[T]):
    def method(self, *args: T, **kwargs: T) -> None: ...

def identity(callback: Callable[P, None]) -> Callable[P, None]:
    return callback

callback = identity(C[Any]().method)
reveal_type(callback)  # revealed: (*args: Any, **kwargs: Any) -> None
static_assert(is_subtype_of(TypeOf[callback], Callable[[], None]))
```

## ParamSpec inference preserves non-gradual residual parameters

Removing a `Concatenate` prefix while inferring a `ParamSpec` also preserves whether the remaining
parameters are gradual:

```py
from typing import Any, Callable, Concatenate, Generic, ParamSpec, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of

P = ParamSpec("P")
T = TypeVar("T")

class C(Generic[T]):
    def method(self, first: int, *args: T, **kwargs: T) -> None: ...

def strip_first(callback: Callable[Concatenate[int, P], None]) -> Callable[P, None]:
    raise NotImplementedError

callback = strip_first(C[Any]().method)
reveal_type(callback)  # revealed: (*args: Any, **kwargs: Any) -> None
static_assert(is_subtype_of(TypeOf[callback], Callable[[], None]))
```

## Gradual class parameters

A callback that accepts `type[Any]` or `type[Unknown]` can accept any class object.

```py
from typing import Any, Callable, TypeVar
from ty_extensions._internal import Unknown

T = TypeVar("T")

def invoke(callback: Callable[[type], T]) -> T:
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

def invoke_any(callback: Callable[[type[Any]], T], cls: type[Any]) -> T:
    return callback(cls)

def _(cls: type[Any]):
    reveal_type(invoke_any(f, cls))  # revealed: int
```

## Inferring gradual tuple returns with concrete bounds

```toml
[environment]
python-version = "3.11"
```

A callback returning `tuple[Any, ...]` satisfies a fixed-length tuple bound because both its
elements and its length are gradual. Inference preserves the callback's return type.

```py
from typing import Any, Callable, TypeVar

Fixed = TypeVar("Fixed", bound=tuple[int])

def get_tuple() -> tuple[Any, ...]:
    return ()

def infer_fixed(callback: Callable[[], Fixed]) -> Fixed:
    return callback()

reveal_type(infer_fixed(get_tuple))  # revealed: tuple[Any, ...]
```

The gradual length can also supply required elements at either end of a variable-length bound.

```py
Prefix = TypeVar("Prefix", bound=tuple[int, *tuple[int, ...]])
Suffix = TypeVar("Suffix", bound=tuple[*tuple[int, ...], int])

def infer_prefix(callback: Callable[[], Prefix]) -> Prefix:
    return callback()

def infer_suffix(callback: Callable[[], Suffix]) -> Suffix:
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
from typing import Any, Callable, TypeVar
from typing_extensions import Unpack

K = TypeVar("K")
V = TypeVar("V")

def infer_pair(callback: Callable[[], tuple[K, V]]) -> tuple[K, V]:
    return callback()

def _(
    callback: Callable[[], tuple[Any, ...]],
    prefix: Callable[[], tuple[int, Unpack[tuple[Any, ...]]]],
    suffix: Callable[[], tuple[Unpack[tuple[Any, ...]], str]],
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

```toml
[environment]
python-version = "3.11"
```

A callback's fixed tuple element can contain an outer type variable and still satisfy a concrete
bound. The gradual segment can be empty, and inference preserves the outer type variable.

```py
from typing import Any, Callable, TypeVar

R = TypeVar("R", bound=tuple[object])
T = TypeVar("T")

def infer_tuple(callback: Callable[[], R]) -> R:
    return callback()

def outer(callback: Callable[[], tuple[list[T], *tuple[Any, ...]]]) -> None:
    reveal_type(infer_tuple(callback))  # revealed: tuple[list[T@outer], *tuple[Any, ...]]
```

## SymPy one-import MRE scaffold (multi-file)

Reduced regression lock for a SymPy overload/protocol shape that can panic in the
overload-assignability path.

```py
from __future__ import annotations

from sympy.polys.compatibility import Domain, IPolys
from typing import Generic, TypeVar, overload

T = TypeVar("T")

class DefaultPrinting:
    pass

class PolyRing(DefaultPrinting, IPolys[T], Generic[T]):
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

from typing import Generic, Protocol, TypeVar, overload

T = TypeVar("T")
S = TypeVar("S")

class Domain(Generic[T]): ...

class IPolys(Protocol[T]):
    @overload
    def clone(
        self,
        symbols: object | None = None,
        domain: None = None,
        order: None = None,
    ) -> IPolys[T]: ...
    @overload
    def clone(
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
