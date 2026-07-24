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

def accepts_callable_and_value(converter: Callable[[T], None], value: T) -> T:
    converter(value)
    return value

@overload
def overloaded_consumer(val: str) -> None: ...
@overload
def overloaded_consumer(val: bytes) -> None: ...
def overloaded_consumer(val: str | bytes) -> None:
    pass

reveal_type(accepts_callable(overloaded_consumer))  # revealed: Never
```

An additional argument of type `T` supplies the return value and constrains the valid
specializations. A `str | bytes` value is accepted because the overload set covers both cases:

```py
def _(string: str, data: bytes, either: str | bytes) -> None:
    reveal_type(accepts_callable_and_value(overloaded_consumer, string))  # revealed: str
    reveal_type(accepts_callable_and_value(overloaded_consumer, data))  # revealed: bytes
    reveal_type(accepts_callable_and_value(overloaded_consumer, either))  # revealed: str | bytes
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
