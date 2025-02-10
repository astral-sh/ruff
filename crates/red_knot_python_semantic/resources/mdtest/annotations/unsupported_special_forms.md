# Unsupported special forms

## Not yet supported

Several special forms are unsupported by red-knot currently. However, we also don't emit
false-positive errors if you use one in an annotation:

```py
from typing_extensions import Self, TypeVarTuple, Unpack, TypeGuard, TypeIs, Concatenate, ParamSpec, TypeAlias, Callable, TypeVar

P = ParamSpec("P")
Ts = TypeVarTuple("Ts")
R_co = TypeVar("R_co", covariant=True)

Alias: TypeAlias = int

def f(*args: Unpack[Ts]) -> tuple[Unpack[Ts]]:
    # TODO: should understand the annotation
    reveal_type(args)  # revealed: tuple

    reveal_type(Alias)  # revealed: @Todo(Unsupported or invalid type in a type expression)

def g() -> TypeGuard[int]: ...
def h() -> TypeIs[int]: ...
def i(callback: Callable[Concatenate[int, P], R_co], *args: P.args, **kwargs: P.kwargs) -> R_co:
    # TODO: should understand the annotation
    reveal_type(args)  # revealed: tuple

    # TODO: should understand the annotation
    reveal_type(kwargs)  # revealed: dict

    return callback(42, *args, **kwargs)

class Foo:
    def method(self, x: Self):
        reveal_type(x)  # revealed: @Todo(Unsupported or invalid type in a type expression)
```

## Inheritance

You can't inherit from most of these. `typing.Callable` is an exception.

```py
from typing import Callable
from typing_extensions import Self, Unpack, TypeGuard, TypeIs, Concatenate

class A(Self): ...  # error: [invalid-base]
class B(Unpack): ...  # error: [invalid-base]
class C(TypeGuard): ...  # error: [invalid-base]
class D(TypeIs): ...  # error: [invalid-base]
class E(Concatenate): ...  # error: [invalid-base]
class F(Callable): ...

reveal_type(F.__mro__)  # revealed: tuple[Literal[F], @Todo(Support for Callable as a base class), Literal[object]]
```

## Subscriptability

Some of these are not subscriptable:

```py
from typing_extensions import Self, TypeAlias

X: TypeAlias[T] = int  # error: [invalid-type-form]

class Foo[T]:
    # error: [invalid-type-form] "Special form `typing.Self` expected no type parameter"
    # error: [invalid-type-form] "Special form `typing.Self` expected no type parameter"
    def method(self: Self[int]) -> Self[int]:
        reveal_type(self)  # revealed: Unknown
```
