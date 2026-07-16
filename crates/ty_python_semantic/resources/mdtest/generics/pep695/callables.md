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

## Overlapping inferred union upper bounds exceeding the solution budget

Even if the precise intersection of two large union upper bounds is small, processing either union
currently exceeds the solution budget before we can discover that intersection.

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
from typing import Callable

def infer_str[T: str](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_int_or_str(value: int | str) -> None: ...

reveal_type(infer_str(consume_int_or_str))  # revealed: str
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

The type variable should be inferred from the first matching overload, rather than unioning
parameter types across all overloads (which would create an unsatisfiable expected type for
contravariant type variables).

```py
from typing import Callable, overload

def accepts_callable[T](converter: Callable[[T], None]) -> T:
    raise NotImplementedError

@overload
def f(val: str) -> None: ...
@overload
def f(val: bytes) -> None: ...
def f(val: str | bytes) -> None:
    pass

reveal_type(accepts_callable(f))  # revealed: str | bytes
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
