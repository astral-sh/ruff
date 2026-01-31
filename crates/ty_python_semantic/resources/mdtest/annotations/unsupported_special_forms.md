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

def first_arg_int(*args: Unpack[tuple[int, Unpack[tuple[str, ...]]]]): ...

first_arg_int(42, "42", "42")  # fine
first_arg_int("not an int", "42", "42")  # TODO: should error
first_arg_int(56, "42", 56)  # TODO: should error
```

## Allowed `Unpack` contexts

We do not yet model every `Unpack` form precisely, but we should not emit false-positive diagnostics
in contexts where `Unpack` is allowed.

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import Callable, Generic, TypeVar, TypeVarTuple, Unpack

T = TypeVar("T")
U = TypeVar("U")
Ts = TypeVarTuple("Ts")
Us = TypeVarTuple("Us")

class Variadic(Generic[Unpack[Ts]]): ...
class Prefix(Generic[T, Unpack[Ts]]): ...
class Suffix(Generic[Unpack[Ts], T]): ...
class Pair(Generic[T, U]): ...
class Triple(Generic[T, U, Unpack[Us]]): ...

def variadic_typevartuple(*args: Unpack[Ts]) -> None:
    reveal_type(args)  # revealed: tuple[@Todo(`Unpack[]` special form), ...]

def variadic_tuple(*args: Unpack[tuple[int, str]]) -> None:
    reveal_type(args)  # revealed: tuple[@Todo(`Unpack[]` special form), ...]

def allowed(
    tuple_fixed: tuple[int, Unpack[tuple[str, bytes]]],
    tuple_variadic: tuple[int, Unpack[tuple[str, ...]], bytes],
    callable_typevartuple: Callable[[int, Unpack[Ts]], None],
    callable_tuple: Callable[[Unpack[tuple[int, str]]], None],
    # TODO: false positives (generic classes using `TypeVarTuple` are not fully supported yet)
    variadic: Variadic[Unpack[tuple[int, str]]],  # error: [not-subscriptable]
    prefix: Prefix[int, Unpack[tuple[str, bytes]]],  # error: [not-subscriptable]
    suffix: Suffix[Unpack[tuple[int, str]], bytes],  # error: [not-subscriptable]
    pair: Pair[Unpack[tuple[int, str]]],
    quoted_pair_argument: Pair["Unpack[tuple[int, str]]"],
    triple: Triple[int, Unpack[tuple[str, bytes]]],  # error: [not-subscriptable]
    quoted_tuple: "tuple[int, Unpack[tuple[str, bytes]]]",
    quoted_pair: "Pair[Unpack[tuple[int, str]]]",
) -> None:
    reveal_type(tuple_fixed)  # revealed: tuple[int, str, bytes]
    reveal_type(tuple_variadic)  # revealed: tuple[int, *tuple[str, ...], bytes]
    reveal_type(callable_typevartuple)  # revealed: (...) -> None
    reveal_type(callable_tuple)  # revealed: (tuple[int, str], /) -> None
    reveal_type(pair)  # revealed: Pair[int, str]
    reveal_type(quoted_pair_argument)  # revealed: Pair[int, str]
    reveal_type(quoted_tuple)  # revealed: tuple[int, str, bytes]
    reveal_type(quoted_pair)  # revealed: Pair[int, str]

def invalid_parameter(invalid: Unpack[tuple[int, str]]) -> None:  # error: [invalid-type-form]
    pass

def invalid_generic(
    non_tuple: Pair[Unpack[int], str],  # error: [invalid-type-form]
    quoted_non_tuple: Pair["Unpack[int]", str],  # error: [invalid-type-form]
    variadic_tuple: Pair[Unpack[tuple[int, ...]], str],  # error: [invalid-type-form]
    quoted_variadic_tuple: Pair["Unpack[tuple[int, ...]]", str],  # error: [invalid-type-form]
) -> None:
    pass
```

## Type expressions

One thing that is supported is error messages for using special forms in type expressions.

```py
from typing_extensions import Unpack, TypeGuard, TypeIs, Concatenate, ParamSpec, Generic

def _(
    a: Unpack,  # error: [invalid-type-form] "`typing.Unpack` requires exactly one argument when used in a parameter annotation"
    b: TypeGuard,  # error: [invalid-type-form] "`typing.TypeGuard` requires exactly one argument when used in a parameter annotation"
    c: TypeIs,  # error: [invalid-type-form] "`typing.TypeIs` requires exactly one argument when used in a parameter annotation"
    d: Concatenate,  # error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a parameter annotation"
    e: ParamSpec,
    f: Generic,  # error: [invalid-type-form] "`typing.Generic` is not allowed in parameter annotations"
) -> None:
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: Unknown

    # error: [invalid-type-form] "Variable of type `ParamSpec` is not allowed in a parameter annotation"
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
