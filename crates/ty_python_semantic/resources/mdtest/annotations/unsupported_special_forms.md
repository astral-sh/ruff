# Unsupported special forms

## Not yet supported

Several special forms are unsupported by ty currently. However, we also don't emit false-positive
errors if you use one in an annotation:

```py
from typing_extensions import Self, TypeVarTuple, Unpack, TypeGuard, TypeIs, Concatenate, ParamSpec, TypeAlias, Callable, TypeVar

P = ParamSpec("P")
Ts = TypeVarTuple("Ts")
R_co = TypeVar("R_co", covariant=True)

def f(*args: Unpack[Ts]) -> tuple[Unpack[Ts]]:
    reveal_type(args)  # revealed: tuple[@Todo(`Unpack[]` special form), ...]
    return args

def g() -> TypeGuard[int]: ...
def i(callback: Callable[Concatenate[int, P], R_co], *args: P.args, **kwargs: P.kwargs) -> R_co:
    reveal_type(args)  # revealed: P@i.args
    reveal_type(kwargs)  # revealed: P@i.kwargs
    return callback(42, *args, **kwargs)

class Foo:
    def method(self, x: Self):
        reveal_type(x)  # revealed: Self@method

def ex2(msg: str):
    def wrapper(fn: Callable[P, R_co]) -> Callable[P, R_co]:
        def wrapped(*args: P.args, **kwargs: P.kwargs) -> R_co:
            print(msg)
            return fn(*args, **kwargs)
        return wrapped
    return wrapper

def ex3(msg: str):
    P = ParamSpec("P")
    def wrapper(fn: Callable[P, R_co]) -> Callable[P, R_co]:
        def wrapped(*args: P.args, **kwargs: P.kwargs) -> R_co:
            print(msg)
            return fn(*args, **kwargs)
        return wrapped
    return wrapper
```

## Type expressions

One thing that is supported is error messages for using special forms in type expressions.

```py
from typing_extensions import Unpack, TypeGuard, TypeIs, Concatenate, ParamSpec, Generic

def _(
    a: Unpack,  # error: [invalid-type-form] "`typing.Unpack` requires exactly one argument when used in a type expression"
    b: TypeGuard,  # error: [invalid-type-form] "`typing.TypeGuard` requires exactly one argument when used in a type expression"
    c: TypeIs,  # error: [invalid-type-form] "`typing.TypeIs` requires exactly one argument when used in a type expression"
    d: Concatenate,  # error: [invalid-type-form] "`typing.Concatenate` requires at least two arguments when used in a type expression"
    e: ParamSpec,
    f: Generic,  # error: [invalid-type-form] "`typing.Generic` is not allowed in type expressions"
) -> None:
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown

    # error: [invalid-type-form] "Variable of type `ParamSpec` is not allowed in a type expression"
    def foo(a_: e) -> None:
        reveal_type(a_)  # revealed: Unknown
```

## Inheritance

You can't inherit from most of these. `typing.Callable` is an exception.

```py
from typing import Callable
from typing_extensions import Self, Unpack, TypeGuard, TypeIs, Concatenate, Generic
from ty_extensions import reveal_mro

class A(Self): ...  # error: [invalid-base]
class B(Unpack): ...  # error: [invalid-base]
class C(TypeGuard): ...  # error: [invalid-base]
class D(TypeIs): ...  # error: [invalid-base]
class E(Concatenate): ...  # error: [invalid-base]
class F(Callable): ...
class G(Generic): ...  # error: [invalid-base] "Cannot inherit from plain `Generic`"

reveal_mro(F)  # revealed: (<class 'F'>, @Todo(Support for Callable as a base class), <class 'object'>)
```

## Subscriptability

```toml
[environment]
python-version = "3.12"
```

Some of these are not subscriptable:

```py
from typing_extensions import Self, TypeAlias, TypeVar

T = TypeVar("T")

# error: [invalid-type-form] "Special form `typing.TypeAlias` expected no type parameter"
X: TypeAlias[T] = int

class Foo[T]:
    # error: [invalid-type-form] "Special form `typing.Self` expected no type parameter"
    # error: [invalid-type-form] "Special form `typing.Self` expected no type parameter"
    def method(self: Self[int]) -> Self[int]:
        reveal_type(self)  # revealed: Unknown
```
