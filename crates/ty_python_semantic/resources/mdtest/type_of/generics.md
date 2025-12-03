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
