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

## Overloaded callable as generic `Callable` argument

An overloaded callable should be assignable to a non-overloaded callable type when the overload set
as a whole is compatible with the target callable.

The type variable should be inferred from the first matching overload, rather than unioning
parameter types across all overloads (which would create an unsatisfiable expected type for
contravariant type variables).

```py
from typing import Callable, TypeVar, overload

T = TypeVar("T")

def accepts_callable(converter: Callable[[T], None]) -> T:
    raise NotImplementedError

@overload
def f(val: str) -> None: ...
@overload
def f(val: bytes) -> None: ...
def f(val: str | bytes) -> None:
    pass

reveal_type(accepts_callable(f))  # revealed: str | bytes
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

## Inference from a bounded callable type variable

A bounded type variable provides its upper bound's callable signature when passed to a generic
function:

```py
from typing import Callable, TypeVar

T = TypeVar("T")
F = TypeVar("F", bound=Callable[[int], str])

def apply(callback: Callable[[int], T]) -> T:
    return callback(1)

def _(callback: F):
    reveal_type(apply(callback))  # revealed: str
```

A class bound provides the signature of its `__call__` method:

```py
class Printer:
    def __call__(self, value: int) -> str:
        return str(value)

P = TypeVar("P", bound=Printer)

def _(callback: P):
    reveal_type(apply(callback))  # revealed: str
    x1: Callable[[str], str] = callback  # error: [invalid-assignment]
    x2: Callable[[int], int] = callback  # error: [invalid-assignment]
```

If `__call__` returns `Self`, its return type is the type variable, not the upper bound:

```py
from typing_extensions import Self

class Clone:
    def __call__(self, value: int) -> Self:
        return self

C = TypeVar("C", bound=Clone)

def _(callback: C) -> C:
    reveal_type(apply(callback))  # revealed: C@_
    return apply(callback)
```

This also applies to constrained type variables:

```py
class OtherClone:
    def __call__(self, value: int) -> Self:
        return self

G = TypeVar("G", Clone, OtherClone)

def _(callback: G) -> G:
    reveal_type(apply(callback))  # revealed: G@_
    return apply(callback)
```

When `__call__` is a classmethod, its receiver is a class-object type, but `Self` still refers to
the original type variable:

```py
class ClassClone:
    @classmethod
    def __call__(cls, value: int) -> Self:
        return cls()

H = TypeVar("H", bound=ClassClone)
I = TypeVar("I", ClassClone, OtherClone)

def _(callback: H) -> H:
    reveal_type(apply(callback))  # revealed: H@_
    return apply(callback)

def _(callback: I) -> I:
    reveal_type(apply(callback))  # revealed: I@_
    x: Callable[[int], str] = callback  # error: [invalid-assignment]
    return apply(callback)
```

Properties that return a callable also preserve `Self`. For constrained type variables, evaluating
the property does not replace `Self` with the concrete constraint:

```py
class PropertyClone:
    @property
    def __call__(self) -> Callable[[int], Self]:
        return lambda _: self

J = TypeVar("J", bound=PropertyClone)
K = TypeVar("K", PropertyClone, OtherClone)

def _(callback: J) -> J:
    reveal_type(apply(callback))  # revealed: J@_
    return apply(callback)

def _(callback: K) -> K:
    reveal_type(apply(callback))  # revealed: K@_
    x: Callable[[int], str] = callback  # error: [invalid-assignment]
    return apply(callback)
```

## Explicit receivers in callable bounds

A type variable's bound should satisfy an explicit receiver annotation just as the concrete type
does:

```py
from typing import Callable, TypeVar

class C:
    def __call__(self: "C", value: int) -> str:
        return str(value)

def _(callback: C):
    x: Callable[[int], str] = callback

F = TypeVar("F", bound=C)

def _(callback: F):
    # TODO: Accept this assignment by checking the receiver constraint under F's declared bound.
    x: Callable[[int], str] = callback  # error: [invalid-assignment]
```

## Callable instances bound as classmethods

A constrained type variable remains assignable to a compatible `Callable` when `__call__` is a
classmethod wrapping a callable instance. We bind the class-object receiver before comparing
signatures:

```py
from typing import Callable, TypeVar

class Invoke:
    def __call__(self, cls: type["Wrapper"], value: int) -> str:
        return str(value)

class Wrapper:
    __call__ = classmethod(Invoke())

class Other:
    def __call__(self, value: int) -> str:
        return str(value)

T = TypeVar("T")
F = TypeVar("F", Wrapper, Other)

def apply(callback: Callable[[int], T]) -> T:
    return callback(1)

def _(callback: Wrapper):
    x: Callable[[int], str] = callback

def _(callback: F):
    reveal_type(apply(callback))  # revealed: str
    x1: Callable[[int], str] = callback
    x2: Callable[[str], str] = callback  # error: [invalid-assignment]
```

## Bound methods returned by callable descriptors

A `__call__` property can return a method bound to another object. Its `Self` refers to that object,
not the bounded or constrained type variable:

```py
from typing import Callable, TypeVar
from typing_extensions import Self
from ty_extensions._internal import TypeOf

class Other:
    def method(self, value: int) -> Self:
        return self

    @classmethod
    def class_method(cls, value: int) -> Self:
        return cls()

other = Other()

class Wrapper:
    @property
    def __call__(self) -> TypeOf[other.method]:
        return other.method

T = TypeVar("T")
F = TypeVar("F", bound=Wrapper)
G = TypeVar("G", Wrapper, Callable[[int], Other])

def apply(callback: Callable[[int], T]) -> T:
    return callback(1)

def _(callback: F) -> Other:
    reveal_type(apply(callback))  # revealed: Other
    return apply(callback)

def _(callback: G) -> Other:
    reveal_type(apply(callback))  # revealed: Other
    return apply(callback)
```

The same applies to a classmethod bound to another class:

```py
class ClassWrapper:
    @property
    def __call__(self) -> TypeOf[Other.class_method]:
        return Other.class_method

H = TypeVar("H", ClassWrapper, Callable[[int], Other])

def _(callback: H) -> Other:
    reveal_type(apply(callback))  # revealed: Other
    return apply(callback)
```

## Constructor bounds in callable inference

A `type[C]` bound provides the signature of `C`'s constructor. We reject assignments with
incompatible parameter or return types:

```py
from typing import Callable, TypeVar

class C:
    def __init__(self, value: int) -> None: ...

T = TypeVar("T")
F = TypeVar("F", bound=type[C])

def apply(callback: Callable[[int], T]) -> T:
    return callback(1)

def _(callback: F):
    reveal_type(apply(callback))  # revealed: C
    x1: Callable[[int], C] = callback
    x2: Callable[[str], C] = callback  # error: [invalid-assignment]
    x3: Callable[[int], str] = callback  # error: [invalid-assignment]
```

A constrained type variable can represent a constructor or a callable with a different return type.
Both return types contribute to inference, and assignments must accept either callable:

```py
G = TypeVar("G", type[C], Callable[[int], str])

def _(callback: G):
    reveal_type(apply(callback))  # revealed: C | str
    x1: Callable[[int], C | str] = callback
    x2: Callable[[int], str] = callback  # error: [invalid-assignment]
```

Constructor compatibility does not establish subtyping: a subclass may require different constructor
arguments.

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

def _(callback: F):
    static_assert(not is_subtype_of(F, Callable[[int], C]))
```

## Function-like callable bounds

A type variable with a function-like callable bound is itself function-like. It remains a subtype
of, and assignable to, its bound:

```py
from typing import TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import CallableTypeOf, is_assignable_to, is_subtype_of

def function(value: int) -> str:
    return str(value)

F = TypeVar("F", bound=CallableTypeOf[function])

def _(callback: F) -> CallableTypeOf[function]:
    static_assert(is_subtype_of(F, CallableTypeOf[function]))
    static_assert(is_assignable_to(F, CallableTypeOf[function]))
    return callback
```

## Recursive callable bounds

We cannot extract a callable signature from a recursive `__call__` attribute that never reaches a
function or a `Callable`. This applies to both concrete instances and bounded type variables:

```py
from typing import Callable, TypeVar
from ty_extensions._internal import CallableTypeOf, RegularCallableTypeOf

class Recursive:
    __call__: "Recursive"

def _(callback: Recursive):
    x: Callable[[int], str] = callback  # error: [invalid-assignment]

F = TypeVar("F", bound=Recursive)

def _(callback: F):
    x1: CallableTypeOf[callback]  # error: [invalid-type-form]
    x2: RegularCallableTypeOf[callback]  # error: [invalid-type-form]
```

Expanding a recursive protocol can change its specialization without reaching a callable signature:

```py
from typing import Protocol

T_co = TypeVar("T_co", covariant=True)

class Growing(Protocol[T_co]):
    @property
    def __call__(self) -> "Growing[list[T_co]]": ...

H = TypeVar("H", bound=Growing[int])

def _(callback: H):
    x: CallableTypeOf[callback]  # error: [invalid-type-form]
```

The same applies to nominal classes whose `__call__` values grow recursively:

```py
from typing import Generic

T = TypeVar("T")

class GrowingInstance(Generic[T]):
    @property
    def __call__(self) -> "GrowingInstance[list[T]]":
        raise NotImplementedError

I = TypeVar("I", bound=GrowingInstance[int])

def _(callback: I):
    x: CallableTypeOf[callback]  # error: [invalid-type-form]
```

The growing reference can pass through another class:

```py
class First(Generic[T]):
    @property
    def __call__(self) -> "Second[list[T]]":
        raise NotImplementedError

class Second(Generic[T]):
    @property
    def __call__(self) -> First[T]:
        raise NotImplementedError

J = TypeVar("J", bound=First[int])

def _(callback: J):
    x: CallableTypeOf[callback]  # error: [invalid-type-form]
```

However, different specializations of the same class can lead to a concrete signature:

```py
class Wrapper(Generic[T]):
    @property
    def __call__(self) -> T:
        raise NotImplementedError

G = TypeVar("G", bound=Wrapper[Wrapper[Callable[[int], str]]])

def apply(callback: Callable[[int], T]) -> T:
    return callback(1)

def _(callback: G):
    reveal_type(apply(callback))  # revealed: str
```

Recursive members other than `__call__` do not prevent conversion, even if their specializations
grow:

```py
class ProtocolWrapper(Protocol[T]):
    @property
    def __call__(self) -> T: ...
    @property
    def unrelated(self) -> "ProtocolWrapper[list[T]]": ...

L = TypeVar("L", bound=ProtocolWrapper[ProtocolWrapper[Callable[[int], str]]])

def _(callback: L):
    reveal_type(apply(callback))  # revealed: str
    x: Callable[[int], str] = callback
```

Recursion in the signature itself does not prevent callable conversion. We do not follow the return
type's `__call__`:

```py
class RecursiveSignature(Generic[T]):
    def __call__(self, value: int) -> "RecursiveSignature[list[T]]":
        raise NotImplementedError

K = TypeVar("K", bound=RecursiveSignature[int])

def _(callback: K):
    reveal_type(apply(callback))  # revealed: RecursiveSignature[list[int]]
```

## Aliases in recursive callable bounds

Aliases in a finite callable wrapper chain preserve its signature, even when unrelated members
recursively refer to the alias:

```py
from typing import Callable, Protocol, TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")

class Wrapper(Protocol[T]):
    @property
    def __call__(self) -> T: ...
    @property
    def unrelated(self) -> "Alias[list[T]]": ...

Alias = TypeAliasType("Alias", Wrapper[T], type_params=(T,))
F = TypeVar("F", bound=Alias[Alias[Callable[[int], str]]])

def apply(callback: Callable[[int], T]) -> T:
    return callback(1)

def _(callback: Alias[Alias[Callable[[int], str]]]):
    reveal_type(apply(callback))  # revealed: str
    x: int = apply(callback)  # error: [invalid-assignment]

def _(callback: F):
    reveal_type(apply(callback))  # revealed: str
    x: int = apply(callback)  # error: [invalid-assignment]
```

An alias whose `__call__` chain grows without reaching a signature cannot be converted to a
callable:

```py
from ty_extensions._internal import CallableTypeOf

T_co = TypeVar("T_co", covariant=True)

class Growing(Protocol[T_co]):
    @property
    def __call__(self) -> "GrowingAlias[list[T_co]]": ...

GrowingAlias = TypeAliasType("GrowingAlias", Growing[T_co], type_params=(T_co,))
G = TypeVar("G", bound=GrowingAlias[int])

def _(callback: GrowingAlias[int]):
    x: CallableTypeOf[callback]  # error: [invalid-type-form]

def _(callback: G):
    x: CallableTypeOf[callback]  # error: [invalid-type-form]
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

## Contradictory bounds from a callback

A callback's parameter and return types can impose incompatible bounds on the same type variable. We
reject the argument even when inference cannot produce a specialization, with or without a declared
return context.

```py
from typing import Any, Callable, TypeVar, overload

T = TypeVar("T")

def f(callback: Callable[[T], T]) -> list[T]:
    raise NotImplementedError

def incompatible(value: int) -> str:
    return str(value)

f(incompatible)  # error: [invalid-argument-type]
result: list[int] = f(incompatible)  # error: [invalid-argument-type]

def compatible(value: int) -> int:
    return value

def gradual(value: Any) -> Any:
    return value

valid: list[int] = f(compatible)
dynamic: list[int] = f(gradual)
```

Overloads do not resolve the contradiction when every alternative has incompatible parameter and
return types.

```py
@overload
def crossed(value: int) -> str: ...
@overload
def crossed(value: str) -> int: ...
def crossed(value: int | str) -> int | str:
    raise NotImplementedError

f(crossed)  # error: [invalid-argument-type]
overloaded: list[int] = f(crossed)  # error: [invalid-argument-type]
```

The same contradiction is diagnosed for generic constructors. A declared specialization can make the
diagnostic more precise, but cannot make the callback compatible.

```py
from typing import Generic
from typing_extensions import Self

class Init(Generic[T]):
    def __init__(self, callback: Callable[[T], T]) -> None: ...

class New(Generic[T]):
    def __new__(cls, callback: Callable[[T], T]) -> Self:
        return super().__new__(cls)

Init(incompatible)  # error: [invalid-argument-type]
# error: [invalid-argument-type] "Expected `(int, /) -> int`"
invalid_init: Init[int] = Init(incompatible)
New(incompatible)  # error: [invalid-argument-type]
# error: [invalid-argument-type] "Expected `(int, /) -> int`"
invalid_new: New[int] = New(incompatible)

reveal_type(Init(compatible))  # revealed: Init[int]
valid_init: Init[int] = Init(compatible)
reveal_type(New(compatible))  # revealed: New[int]
valid_new: New[int] = New(compatible)
```
