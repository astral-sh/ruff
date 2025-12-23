# `type[T]`

`type[T]` with a type variable represents the class objects of `T`.

```toml
[environment]
python-version = "3.13"
```

## Basic

The meta-type of a typevar is `type[T]`.

```py
def _[T](x: T):
    reveal_type(type(x))  # revealed: type[T@_]
```

`type[T]` with an unbounded type variable represents any subclass of `object`.

```py
def unbounded[T](x: type[T]) -> T:
    reveal_type(x)  # revealed: type[T@unbounded]
    reveal_type(x.__repr__)  # revealed: def __repr__(self) -> str
    reveal_type(x.__init__)  # revealed: def __init__(self) -> None
    reveal_type(x.__qualname__)  # revealed: str
    reveal_type(x())  # revealed: T@unbounded

    return x()
```

`type[T]` with an upper bound of `T: A` represents any subclass of `A`.

```py
class A:
    x: str

    def __init__(self, value: str): ...

class B(A): ...
class C: ...

def upper_bound[T: A](x: type[T]) -> T:
    reveal_type(x)  # revealed: type[T@upper_bound]
    reveal_type(x.__qualname__)  # revealed: str
    reveal_type(x("hello"))  # revealed: T@upper_bound

    return x("hello")

reveal_type(upper_bound(A))  # revealed: A
reveal_type(upper_bound(B))  # revealed: B

# error: [invalid-argument-type] "Argument to function `upper_bound` is incorrect: Argument type `C` does not satisfy upper bound `A` of type variable `T`"
upper_bound(C)
```

`type[T]` with a constraints `T: (A, B)` represents exactly the class object `A`, or exactly `B`:

```py
def constrained[T: (int, str)](x: type[T]) -> T:
    reveal_type(x)  # revealed: type[T@constrained]
    reveal_type(x.__qualname__)  # revealed: str
    reveal_type(x("hello"))  # revealed: T@constrained

    return x("hello")

reveal_type(constrained(int))  # revealed: int
reveal_type(constrained(str))  # revealed: str

# error: [invalid-argument-type] "Argument to function `constrained` is incorrect: Argument type `A` does not satisfy constraints (`int`, `str`) of type variable `T`"
constrained(A)
```

`type[T]` with a union upper bound `T: A | B` represents the metatype of a type variable `T` where
`T` can be solved to any subtype of `A` or any subtype of `B`. It behaves similarly to a type
variable that can be solved to any subclass of `A` or any subclass of `B`. Since all classes are
instances of `type`, attributes on instances of `type` like `__name__` and `__qualname__` should
still be accessible:

```py
class Replace: ...
class Multiply: ...

def union_bound[T: Replace | Multiply](x: type[T]) -> T:
    reveal_type(x)  # revealed: type[T@union_bound]
    # All classes have __name__ and __qualname__ from type's metaclass
    reveal_type(x.__name__)  # revealed: str
    reveal_type(x.__qualname__)  # revealed: str
    reveal_type(x())  # revealed: T@union_bound

    return x()

reveal_type(union_bound(Replace))  # revealed: Replace
reveal_type(union_bound(Multiply))  # revealed: Multiply
```

## Union

```py
from ty_extensions import Intersection, Unknown

def _[T: int](x: type | type[T]):
    reveal_type(x())  # revealed: Any

def _[T: int](x: type[int] | type[T]):
    reveal_type(x())  # revealed: int

def _[T](x: type[int] | type[T]):
    reveal_type(x())  # revealed: int | T@_
```

## Narrowing

```py
from typing import TypeVar

class A: ...

def narrow_a[B: A](a: A, b: B):
    type_of_a = type(a)

    reveal_type(a)  # revealed: A
    reveal_type(type_of_a)  # revealed: type[A]

    if isinstance(a, type(b)):
        reveal_type(a)  # revealed: B@narrow_a

    if issubclass(type_of_a, type(b)):
        reveal_type(type_of_a)  # revealed: type[B@narrow_a]
```

## `__class__`

```py
from typing import Self

class A:
    def copy(self: Self) -> Self:
        reveal_type(self.__class__)  # revealed: type[Self@copy]
        reveal_type(self.__class__())  # revealed: Self@copy
        return self.__class__()
```

## Subtyping

A class `A` is a subtype of `type[T]` if any instance of `A` is a subtype of `T`.

```py
from typing import Any, Callable, Protocol
from ty_extensions import is_assignable_to, is_subtype_of, is_disjoint_from, static_assert

class Callback[T](Protocol):
    def __call__(self, *args, **kwargs) -> T: ...

def _[T](_: T):
    static_assert(not is_subtype_of(type[T], T))
    static_assert(not is_subtype_of(T, type[T]))
    static_assert(not is_disjoint_from(type[T], T))
    static_assert(not is_disjoint_from(type[type], type))

    static_assert(is_subtype_of(type[T], type[T]))
    static_assert(not is_disjoint_from(type[T], type[T]))

    static_assert(is_assignable_to(type[T], Callable[..., T]))
    static_assert(not is_disjoint_from(type[T], Callable[..., T]))

    static_assert(is_assignable_to(type[T], Callable[..., T] | Callable[..., Any]))
    static_assert(not is_disjoint_from(type[T], Callable[..., T] | Callable[..., Any]))

    static_assert(not is_assignable_to(type[T], Callback[int]))
    static_assert(not is_disjoint_from(type[T], Callback[int]))

def _[T: int](_: T):
    static_assert(not is_subtype_of(type[T], T))
    static_assert(not is_subtype_of(T, type[T]))
    static_assert(is_disjoint_from(type[T], T))

    static_assert(not is_subtype_of(type[T], int))
    static_assert(not is_subtype_of(int, type[T]))
    static_assert(is_disjoint_from(type[T], int))

    static_assert(not is_subtype_of(type[int], type[T]))
    static_assert(is_subtype_of(type[T], type[int]))
    static_assert(not is_disjoint_from(type[T], type[int]))

    static_assert(is_subtype_of(type[T], type[int] | None))
    static_assert(not is_disjoint_from(type[T], type[int] | None))

    static_assert(is_subtype_of(type[T], type[T]))
    static_assert(not is_disjoint_from(type[T], type[T]))

    static_assert(is_assignable_to(type[T], Callable[..., T]))
    static_assert(not is_disjoint_from(type[T], Callable[..., T]))

    static_assert(is_assignable_to(type[T], Callable[..., T] | Callable[..., Any]))
    static_assert(not is_disjoint_from(type[T], Callable[..., T] | Callable[..., Any]))

    static_assert(is_assignable_to(type[T], Callback[int]))
    static_assert(not is_disjoint_from(type[T], Callback[int]))

    static_assert(is_assignable_to(type[T], Callback[int] | Callback[Any]))
    static_assert(not is_disjoint_from(type[T], Callback[int] | Callback[Any]))

    static_assert(is_subtype_of(type[T], type[T] | None))
    static_assert(not is_disjoint_from(type[T], type[T] | None))

    static_assert(is_subtype_of(type[T], type[T] | type[float]))
    static_assert(not is_disjoint_from(type[T], type[T] | type[float]))

def _[T: (int, str)](_: T):
    static_assert(not is_subtype_of(type[T], T))
    static_assert(not is_subtype_of(T, type[T]))
    static_assert(is_disjoint_from(type[T], T))

    static_assert(is_subtype_of(type[T], type[T]))
    static_assert(not is_disjoint_from(type[T], type[T]))

    static_assert(is_assignable_to(type[T], Callable[..., T]))
    static_assert(not is_disjoint_from(type[T], Callable[..., T]))

    static_assert(is_assignable_to(type[T], Callable[..., T] | Callable[..., Any]))
    static_assert(not is_disjoint_from(type[T], Callable[..., T] | Callable[..., Any]))

    static_assert(not is_assignable_to(type[T], Callback[int]))
    static_assert(not is_disjoint_from(type[T], Callback[int]))

    static_assert(is_assignable_to(type[T], Callback[int | str]))
    static_assert(not is_disjoint_from(type[T], Callback[int] | Callback[str]))

    static_assert(is_subtype_of(type[T], type[T] | None))
    static_assert(not is_disjoint_from(type[T], type[T] | None))

    static_assert(is_subtype_of(type[T], type[T] | type[float]))
    static_assert(not is_disjoint_from(type[T], type[T] | type[float]))

    static_assert(not is_subtype_of(type[T], type[int]))
    static_assert(not is_subtype_of(type[int], type[T]))
    static_assert(not is_subtype_of(type[T], type[str]))
    static_assert(not is_subtype_of(type[str], type[T]))
    static_assert(not is_disjoint_from(type[T], type[int]))
    static_assert(not is_disjoint_from(type[T], type[str]))

    static_assert(is_subtype_of(type[T], type[int] | type[str]))
    static_assert(is_subtype_of(type[T], type[int | str]))
    static_assert(not is_disjoint_from(type[T], type[int | str]))
    static_assert(not is_disjoint_from(type[T], type[int] | type[str]))

def _[T: (int | str, int)](_: T):
    static_assert(is_subtype_of(type[int], type[T]))
    static_assert(not is_disjoint_from(type[int], type[T]))
```

```py
class X[T]:
    value: T

    def get(self) -> T:
        return self.value

def _[T](x: X[type[T]]):
    reveal_type(x.get())  # revealed: type[T@_]
```

## Generic Type Inference

```py
def f1[T](x: type[T]) -> type[T]:
    return x

reveal_type(f1(int))  # revealed: type[int]
reveal_type(f1(object))  # revealed: type

def f2[T](x: T) -> type[T]:
    return type(x)

reveal_type(f2(int(1)))  # revealed: type[int]
reveal_type(f2(object()))  # revealed: type

# TODO: This should reveal `type[Literal[1]]`.
reveal_type(f2(1))  # revealed: type[Unknown]

def f3[T](x: type[T]) -> T:
    return x()

reveal_type(f3(int))  # revealed: int
reveal_type(f3(object))  # revealed: object
```

## Default Parameter

```py
from typing import Any

class Foo[T]: ...

# TODO: This should not error.
# error: [invalid-parameter-default] "Default value of type `<class 'Foo'>` is not assignable to annotated parameter type `type[T@f]`"
def f[T: Foo[Any]](x: type[T] = Foo): ...
```

## Display of generic `type[]` types

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generic, TypeVar

class Foo[T]: ...

S = TypeVar("S")

class Bar(Generic[S]): ...

def _(x: Foo[int], y: Bar[str], z: list[bytes]):
    reveal_type(type(x))  # revealed: type[Foo[int]]
    reveal_type(type(y))  # revealed: type[Bar[str]]
    reveal_type(type(z))  # revealed: type[list[bytes]]
```

## Checking generic `type[]` types

```toml
[environment]
python-version = "3.12"
```

```py
class C[T]:
    pass

class D[T]:
    pass

var: type[C[int]] = C[int]
var: type[C[int]] = D[int]  # error: [invalid-assignment] "Object of type `<class 'D[int]'>` is not assignable to `type[C[int]]`"
```

However, generic `Protocol` classes are still TODO:

```py
from typing import Protocol

class Proto[U](Protocol):
    def some_method(self): ...

# TODO: should be error: [invalid-assignment]
var: type[Proto[int]] = C[int]

def _(p: type[Proto[int]]):
    reveal_type(p)  # revealed: type[@Todo(type[T] for protocols)]
```

## Generic `@final` classes

```toml
[environment]
python-version = "3.13"
```

An unspecialized generic final class object is assignable to its default-specialized `type[]` type
(which is actually internally simplified to a GenericAlias type, since there cannot be subclasses.)

```py
from typing import final

@final
class P[T]:
    x: T

def expects_type_p(x: type[P]):
    pass

def expects_type_p_of_int(x: type[P[int]]):
    pass

# OK, the default specialization of `P` is assignable to `type[P[Unknown]]`
expects_type_p(P)

# Also OK, because `P[int]` and `P[str]` are both assignable to `P[Unknown]`
expects_type_p(P[int])
expects_type_p(P[str])

# Also OK, because the default specialization is `P[Unknown]` which is assignable to `P[int]`
expects_type_p_of_int(P)
expects_type_p_of_int(P[int])

# Not OK, because `P[str]` is not assignable to `P[int]`
expects_type_p_of_int(P[str])  # error: [invalid-argument-type]
```

The same principles apply when typevar defaults are used, but the results are a bit different
because the default-specialization is no longer a forgiving `Unknown` type:

```py
@final
class P[T = str]:
    x: T

def expects_type_p(x: type[P]):
    pass

def expects_type_p_of_int(x: type[P[int]]):
    pass

def expects_type_p_of_str(x: type[P[str]]):
    pass

# OK, the default specialization is now `P[str]`, but we have the default specialization on both
# sides, so it is assignable.
expects_type_p(P)

# Also OK if the explicit specialization lines up with the default, in either direction:
expects_type_p(P[str])
expects_type_p_of_str(P)
expects_type_p_of_str(P[str])

# Not OK if the specializations don't line up:
expects_type_p(P[int])  # error: [invalid-argument-type]
expects_type_p_of_int(P[str])  # error: [invalid-argument-type]
expects_type_p_of_int(P)  # error: [invalid-argument-type]
expects_type_p_of_str(P[int])  # error: [invalid-argument-type]
```

This also works with `ParamSpec`:

```py
@final
class C[**P]: ...

def expects_type_c(f: type[C]): ...
def expects_type_c_of_int_and_str(x: type[C[int, str]]): ...

# OK, the unspecialized `C` is assignable to `type[C[...]]`
expects_type_c(C)

# Also OK, any specialization is assignable to the unspecialized `C`
expects_type_c(C[int])
expects_type_c(C[str, int, bytes])

# Ok, the unspecialized `C` is assignable to `type[C[int, str]]`
expects_type_c_of_int_and_str(C)

# Also OK, the specialized `C[int, str]` is assignable to `type[C[int, str]]`
expects_type_c_of_int_and_str(C[int, str])

# TODO: these should be errors
expects_type_c_of_int_and_str(C[str])
expects_type_c_of_int_and_str(C[int, str, bytes])
expects_type_c_of_int_and_str(C[str, int])
```

And with a `ParamSpec` that has a default:

```py
@final
class C[**P = [int, str]]: ...

def expects_type_c_default(f: type[C]): ...
def expects_type_c_default_of_int(f: type[C[int]]): ...
def expects_type_c_default_of_int_str(f: type[C[int, str]]): ...

expects_type_c_default(C)
expects_type_c_default(C[int, str])
expects_type_c_default_of_int(C)
expects_type_c_default_of_int(C[int])
expects_type_c_default_of_int_str(C)
expects_type_c_default_of_int_str(C[int, str])

# TODO: these should be errors
expects_type_c_default(C[int])
expects_type_c_default_of_int(C[str])
expects_type_c_default_of_int_str(C[str, int])
```
