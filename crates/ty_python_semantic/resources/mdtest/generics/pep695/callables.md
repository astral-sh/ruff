# Generic callables: PEP 695 syntax

```toml
[environment]
python-version = "3.12"
```

## Callables can be generic

Many items that are callable can also be generic. Generic functions are the most obvious example:

```py
from typing import Callable
from ty_extensions import generic_context

def identity[T](t: T) -> T:
    return t

# revealed: ty_extensions.GenericContext[T@identity]
reveal_type(generic_context(identity))
# revealed: Literal[1]
reveal_type(identity(1))

def identity2[**P, T](c: Callable[P, T]) -> Callable[P, T]:
    return c

# revealed: ty_extensions.GenericContext[P@identity2, T@identity2]
reveal_type(generic_context(identity2))
# revealed: [T](t: T) -> T
reveal_type(identity2(identity))
```

Generic classes are another example, since you invoke the class to instantiate it:

```py
class C[T]:
    t: T  # invariant

    def __init__(self, t: T) -> None: ...

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: C[int]
reveal_type(C(1))
```

When we coerce a generic callable into a `Callable` type, it remembers that it is generic:

```py
from ty_extensions import into_regular_callable

# revealed: [T](t: T) -> T
reveal_type(into_regular_callable(identity))
# revealed: ty_extensions.GenericContext[T@identity]
reveal_type(generic_context(into_regular_callable(identity)))
# revealed: Literal[1]
reveal_type(into_regular_callable(identity)(1))

# revealed: [**P, T](c: (**P) -> T) -> ((**P) -> T)
reveal_type(into_regular_callable(identity2))
# revealed: ty_extensions.GenericContext[P@identity2, T@identity2]
reveal_type(generic_context(into_regular_callable(identity2)))
# revealed: [T](t: T) -> T
reveal_type(into_regular_callable(identity2)(identity))

# revealed: [T](t: T) -> C[T]
reveal_type(into_regular_callable(C))
# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(into_regular_callable(C)))
# revealed: C[int]
reveal_type(into_regular_callable(C)(1))
```

## Naming a generic `Callable`: type aliases

The easiest way to refer to a generic `Callable` type directly is via a type alias:

```py
from typing import Callable
from ty_extensions import generic_context

type IdentityCallable[T] = Callable[[T], T]

def decorator_factory[T]() -> IdentityCallable[T]:
    def decorator[T](fn: T) -> T:
        return fn
    # revealed: ty_extensions.GenericContext[T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

# revealed: [T'return](T'return, /) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: Literal[1]
reveal_type(decorator_factory()(1))
```

## Naming a generic `Callable` with paramspecs: type aliases

The same pattern holds if the callable involves a paramspec.

```py
from typing import Callable
from ty_extensions import generic_context

type IdentityCallable[**P, T] = Callable[[Callable[P, T]], Callable[P, T]]

def decorator_factory[**P, T]() -> IdentityCallable[P, T]:
    def decorator[**P, T](fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity[T](t: T) -> T:
    return t

# revealed: [**P'return, T'return]((**P'return) -> T'return, /) -> ((**P'return) -> T'return)
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[P'return@decorator_factory, T'return@decorator_factory]
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
from ty_extensions import generic_context

def decorator_factory[T]() -> Callable[[T], T]:
    def decorator[T](fn: T) -> T:
        return fn
    # revealed: ty_extensions.GenericContext[T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

# revealed: [T'return](T'return, /) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: Literal[1]
reveal_type(decorator_factory()(1))
```

Nested returned callables bind typevars on the innermost returned callable that covers all of their
occurrences:

```py
from typing import Callable

def nested_factory[T]() -> Callable[[], Callable[[T], T]]:
    raise NotImplementedError

# revealed: () -> ([T'return](T'return, /) -> T'return)
reveal_type(nested_factory())
# revealed: [T'return](T'return, /) -> T'return
reveal_type(nested_factory()())
# revealed: Literal[1]
reveal_type(nested_factory()()(1))
```

Nested returned callables can still have distinct typevars rebound on multiple callable levels:

```py
from typing import Callable

def curried_factory[A, B]() -> Callable[[A], Callable[[B], B]]:
    raise NotImplementedError

# revealed: [A'return](A'return, /) -> ([B'return](B'return, /) -> B'return)
reveal_type(curried_factory())
# revealed: [B'return](B'return, /) -> B'return
reveal_type(curried_factory()(1))
# revealed: Literal["x"]
reveal_type(curried_factory()(1)("x"))
```

A callable nested inside a returned callable's parameter type is part of the surrounding callable's
signature, so the surrounding callable binds the typevar:

```py
from typing import Callable

def accepts_identity[T]() -> Callable[[Callable[[T], T]], int]:
    raise NotImplementedError

# revealed: [T'return]((T'return, /) -> T'return, /) -> int
reveal_type(accepts_identity())
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable[T](t: T) -> Callable[[T], T]:
    raise NotImplementedError

# revealed: ty_extensions.GenericContext[T@outside_callable]
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
from ty_extensions import generic_context

def decorator_factory[**P, T]() -> Callable[[Callable[P, T]], Callable[P, T]]:
    def decorator[**P, T](fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity[T](t: T) -> T:
    return t

# revealed: [**P'return, T'return]((**P'return) -> T'return, /) -> ((**P'return) -> T'return)
reveal_type(decorator_factory())
# revealed: ty_extensions.GenericContext[P'return@decorator_factory, T'return@decorator_factory]
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

# revealed: ty_extensions.GenericContext[P@outside_callable, T@outside_callable]
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

## Generic callable returned from a higher-order call

When a generic callable flows through a higher-order call into the returned callable, the returned
callable should remain generic instead of leaking the callee's type variables.

```py
from typing import Callable

def higher[A, B](f: Callable[[A], B]) -> Callable[[A], B]:
    raise NotImplementedError

def identity[T](x: T) -> T:
    return x

# revealed: [A'return](A'return, /) -> A'return
reveal_type(higher(identity))
# revealed: Literal[1]
reveal_type(higher(identity)(1))
# revealed: Literal["x"]
reveal_type(higher(identity)("x"))
```

Returned callables nested in a higher-order call result use the same innermost-cover rule as
signature-based returned callables.

```py
from typing import Callable

def nested_higher[A, B](f: Callable[[A], B]) -> Callable[[], Callable[[A], B]]:
    raise NotImplementedError

def accepts_higher[A, B](f: Callable[[A], B]) -> Callable[[Callable[[A], B]], int]:
    raise NotImplementedError

def identity[T](x: T) -> T:
    return x

# revealed: () -> ([A'return](A'return, /) -> A'return)
reveal_type(nested_higher(identity))
# revealed: [A'return](A'return, /) -> A'return
reveal_type(nested_higher(identity)())
# revealed: Literal[1]
reveal_type(nested_higher(identity)()(1))

# revealed: [A'return]((A'return, /) -> A'return, /) -> int
reveal_type(accepts_higher(identity))
```

Multiple returned callable occurrences can be used independently after rebinding.

```py
from typing import Callable

def duplicated[A, B](f: Callable[[A], B]) -> tuple[Callable[[A], B], Callable[[A], B]]:
    raise NotImplementedError

def identity[T](x: T) -> T:
    return x

first, second = duplicated(identity)

# revealed: [A'return](A'return, /) -> A'return
reveal_type(first)
# revealed: [A'return](A'return, /) -> A'return
reveal_type(second)
# revealed: Literal[1]
reveal_type(first(1))
# revealed: Literal["x"]
reveal_type(second("x"))
```

Type variables from enclosing generic contexts should remain in that context instead of becoming
returned-callable-local.

```py
from typing import Callable

def identity[T](x: T) -> T:
    return x

def outer[T](value: T) -> None:
    def make[U](f: Callable[[U], object]) -> Callable[[U], T]:
        raise NotImplementedError

    # TODO (optional): Could this be `(object, /) -> T@outer`? Passing `identity`
    # generates `U@make ≤ T@identity ∧ T@identity ≤ object`, but the `object`-only
    # upper bound is currently ignored when solving, so `U@make` falls back to `Unknown`.
    # The important regression here is that `T@outer` is not rebound as callable-local.
    # revealed: (Unknown, /) -> T@outer
    reveal_type(make(identity))

class Box[T]:
    def method[A, B](self, f: Callable[[A], B]) -> Callable[[A], T]:
        raise NotImplementedError

    def test(self) -> None:
        # revealed: [A'return](A'return, /) -> T@Box
        reveal_type(self.method(identity))
```

## Returned callable typevars remain correlated with the surrounding return type

If a typevar appears both inside and outside of a returned callable, the callable occurrence should
not be split into an independent returned-callable typevar. The two positions are correlated by the
source return type.

```py
from typing import Callable

def identity[T](x: T) -> T:
    return x

def pair[X, Y](f: Callable[[X], Y]) -> tuple[Y, Callable[[X], Y]]:
    raise NotImplementedError

# revealed: tuple[X@pair, (X@pair, /) -> X@pair]
reveal_type(pair(identity))
```

## Generic callable arguments do not hide real argument errors

A generic callable argument should not hide errors from a specialization chosen by other arguments.

```py
from typing import Callable

def identity[A](x: A) -> A:
    return x

def same[T](c: Callable[[T], T], x: list[T], y: list[T]) -> T:
    raise NotImplementedError

reveal_type(same(identity, [1], ["x"]))  # revealed: int | str

ints: list[int] = [1]
strings: list[str] = ["x"]

# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(same(identity, ints, strings))  # revealed: int | str
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

# revealed: [X'return](X'return, /) -> ([Y'return](Y'return, /) -> Y'return)
reveal_type(partial(partial, drop))
# revealed: Literal["x"]
reveal_type(partial(partial, drop)(1)("x"))
# revealed: Literal[1]
reveal_type(partial(partial, drop)("x")(1))
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
