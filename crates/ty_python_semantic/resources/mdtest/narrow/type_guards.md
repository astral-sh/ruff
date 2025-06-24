# User-defined type guards

User-defined type guards are functions of which the return type is either `TypeGuard[...]` or
`TypeIs[...]`.

## Display

```py
from ty_extensions import Intersection, Not, TypeOf
from typing_extensions import TypeGuard, TypeIs

def _(
    a: TypeGuard[str],
    b: TypeIs[str | int],
    c: TypeGuard[Intersection[complex, Not[int], Not[float]]],
    d: TypeIs[tuple[TypeOf[bytes]]],
    e: TypeGuard,  # error: [invalid-type-form]
    f: TypeIs,  # error: [invalid-type-form]
):
    # TODO: Should be `TypeGuard[str]`
    reveal_type(a)  # revealed: @Todo(`TypeGuard[]` special form)
    reveal_type(b)  # revealed: TypeIs[str | int]
    # TODO: Should be `TypeGuard[complex & ~int & ~float]`
    reveal_type(c)  # revealed: @Todo(`TypeGuard[]` special form)
    reveal_type(d)  # revealed: TypeIs[tuple[<class 'bytes'>]]
    reveal_type(e)  # revealed: Unknown
    reveal_type(f)  # revealed: Unknown

# TODO: error: [invalid-return-type] "Function always implicitly returns `None`, which is not assignable to return type `TypeGuard[str]`"
def _(a) -> TypeGuard[str]: ...

# error: [invalid-return-type] "Function always implicitly returns `None`, which is not assignable to return type `TypeIs[str]`"
def _(a) -> TypeIs[str]: ...
def f(a) -> TypeGuard[str]:
    return True

def g(a) -> TypeIs[str]:
    return True

def _(a: object):
    # TODO: Should be `TypeGuard[str @ a]`
    reveal_type(f(a))  # revealed: @Todo(`TypeGuard[]` special form)
    reveal_type(g(a))  # revealed: TypeIs[str @ a]
```

## Parameters

A user-defined type guard must accept at least one positional argument (in addition to `self`/`cls`
for non-static methods).

```pyi
from typing_extensions import TypeGuard, TypeIs

# TODO: error: [invalid-type-guard-definition]
def _() -> TypeGuard[str]: ...

# TODO: error: [invalid-type-guard-definition]
def _(**kwargs) -> TypeIs[str]: ...

class _:
    # fine
    def _(self, /, a) -> TypeGuard[str]: ...
    @classmethod
    def _(cls, a) -> TypeGuard[str]: ...
    @staticmethod
    def _(a) -> TypeIs[str]: ...

    # errors
    def _(self) -> TypeGuard[str]: ...  # TODO: error: [invalid-type-guard-definition]
    def _(self, /, *, a) -> TypeGuard[str]: ...  # TODO: error: [invalid-type-guard-definition]
    @classmethod
    def _(cls) -> TypeIs[str]: ...  # TODO: error: [invalid-type-guard-definition]
    @classmethod
    def _() -> TypeIs[str]: ...  # TODO: error: [invalid-type-guard-definition]
    @staticmethod
    def _(*, a) -> TypeGuard[str]: ...  # TODO: error: [invalid-type-guard-definition]
```

For `TypeIs` functions, the narrowed type must be assignable to the declared type of that parameter,
if any.

```pyi
from typing import Any
from typing_extensions import TypeIs

def _(a: object) -> TypeIs[str]: ...
def _(a: Any) -> TypeIs[str]: ...
def _(a: tuple[object]) -> TypeIs[tuple[str]]: ...
def _(a: str | Any) -> TypeIs[str]: ...
def _(a) -> TypeIs[str]: ...

# TODO: error: [invalid-type-guard-definition]
def _(a: int) -> TypeIs[str]: ...

# TODO: error: [invalid-type-guard-definition]
def _(a: bool | str) -> TypeIs[int]: ...
```

## Arguments to special forms

`TypeGuard` and `TypeIs` accept exactly one type argument.

```py
from typing_extensions import TypeGuard, TypeIs

a = 123

# TODO: error: [invalid-type-form]
def f(_) -> TypeGuard[int, str]: ...

# error: [invalid-type-form] "Special form `typing.TypeIs` expected exactly one type parameter"
# error: [invalid-type-form] "Variable of type `Literal[123]` is not allowed in a type expression"
def g(_) -> TypeIs[a, str]: ...

# TODO: Should be `Unknown`
reveal_type(f(0))  # revealed: @Todo(`TypeGuard[]` special form)
reveal_type(g(0))  # revealed: Unknown
```

## Return types

All code paths in a type guard function must return booleans.

```py
from typing_extensions import Literal, TypeGuard, TypeIs, assert_never

def _(a: object, flag: bool) -> TypeGuard[str]:
    if flag:
        return 0

    # TODO: error: [invalid-return-type] "Return type does not match returned value: expected `TypeIs[str]`, found `Literal["foo"]`"
    return "foo"

# error: [invalid-return-type] "Function can implicitly return `None`, which is not assignable to return type `TypeIs[str]`"
def f(a: object, flag: bool) -> TypeIs[str]:
    if flag:
        # error: [invalid-return-type] "Return type does not match returned value: expected `TypeIs[str]`, found `float`"
        return 1.2

def g(a: Literal["foo", "bar"]) -> TypeIs[Literal["foo"]]:
    if a == "foo":
        # Logically wrong, but allowed regardless
        return False

    return False
```

## Invalid calls

```py
from typing import Any
from typing_extensions import TypeGuard, TypeIs

def f(a: object) -> TypeGuard[str]:
    return True

def g(a: object) -> TypeIs[int]:
    return True

def _(d: Any):
    if f():  # error: [missing-argument]
        ...

    # TODO: no error, once we support splatted call args
    if g(*d):  # error: [missing-argument]
        ...

    if f("foo"):  # TODO: error: [invalid-type-guard-call]
        ...

    if g(a=d):  # error: [invalid-type-guard-call]
        ...
```

## Narrowing

```py
from typing import Any
from typing_extensions import TypeGuard, TypeIs

class Foo: ...
class Bar: ...

def guard_foo(a: object) -> TypeGuard[Foo]:
    return True

def is_bar(a: object) -> TypeIs[Bar]:
    return True

def _(a: Foo | Bar):
    if guard_foo(a):
        # TODO: Should be `Foo`
        reveal_type(a)  # revealed: Foo | Bar
    else:
        reveal_type(a)  # revealed: Foo | Bar

    if is_int(a):
        reveal_type(a)  # revealed: Foo
    else:
        reveal_type(a)  # revealed: Foo & ~Bar
```

Attribute and subscript narrowing is supported:

```py
from typing_extensions import Any, Generic, Protocol, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    v: T

def _(a: tuple[str, int] | tuple[int, str], c: C[Any]):
    # TODO: Should be `TypeGuard[str @ a[1]]`
    if reveal_type(guard_str(a[1])):  # revealed: @Todo(`TypeGuard[]` special form)
        # TODO: Should be `tuple[int, str]`
        reveal_type(a)  # revealed: tuple[str, int] | tuple[int, str]
        # TODO: Should be `str`
        reveal_type(a[1])  # revealed: int | str

    if reveal_type(is_int(a[0])):  # revealed: TypeIs[int @ a[0]]
        # TODO: Should be `tuple[int, str]`
        reveal_type(a)  # revealed: tuple[str, int] | tuple[int, str]
        reveal_type(a[0])  # revealed: int

    # TODO: Should be `TypeGuard[str @ c.v]`
    if reveal_type(guard_str(c.v)):  # revealed: @Todo(`TypeGuard[]` special form)
        reveal_type(c)  # revealed: C[Any]
        # TODO: Should be `str`
        reveal_type(c.v)  # revealed: Any

    if reveal_type(is_int(c.v)):  # revealed: TypeIs[int @ c.v]
        reveal_type(c)  # revealed: C[Any]
        reveal_type(c.v)  # revealed: Any & int
```

Indirect usage is supported within the same scope:

```py
def _(a: str | int):
    b = guard_str(a)
    c = is_int(a)

    reveal_type(a)  # revealed: str | int
    # TODO: Should be `TypeGuard[str @ a]`
    reveal_type(b)  # revealed: @Todo(`TypeGuard[]` special form)
    reveal_type(c)  # revealed: TypeIs[int @ a]

    if b:
        # TODO should be `str`
        reveal_type(a)  # revealed: str | int
    else:
        reveal_type(a)  # revealed: str | int

    if c:
        # TODO should be `int`
        reveal_type(a)  # revealed: str | int
    else:
        # TODO should be `str & ~int`
        reveal_type(a)  # revealed: str | int
```

Further writes to the narrowed place invalidate the narrowing:

```py
def _(x: str | int, flag: bool) -> None:
    b = is_int(x)
    reveal_type(b)  # revealed: TypeIs[int @ x]

    if flag:
        x = ""

    if b:
        reveal_type(x)  # revealed: str | int
```

The `TypeIs` type remains effective across generic boundaries:

```py
from typing_extensions import TypeVar, reveal_type

T = TypeVar("T")

class Foo: ...
class Bar: ...

def f(v: object) -> TypeIs[Bar]:
    return True

def g(v: T) -> T:
    return v

def _(a: Foo):
    # `reveal_type()` has the type `[T]() -> T`
    if reveal_type(f(a)):  # revealed: TypeIs[Bar @ a]
        reveal_type(a)  # revealed: Foo & Bar

    if g(f(a)):
        reveal_type(a)  # revealed: Foo & Bar
```

## `TypeGuard` special cases

```py
from typing import Any
from typing_extensions import TypeGuard, TypeIs

def guard_int(a: object) -> TypeGuard[int]:
    return True

def is_int(a: object) -> TypeIs[int]:
    return True

def does_not_narrow_in_negative_case(a: str | int):
    if not guard_int(a):
        # TODO: Should be `str`
        reveal_type(a)  # revealed: str | int
    else:
        reveal_type(a)  # revealed: str | int

def narrowed_type_must_be_exact(a: object, b: bool):
    if guard_int(b):
        # TODO: Should be `int`
        reveal_type(b)  # revealed: bool

    if isinstance(a, bool) and is_int(a):
        reveal_type(a)  # revealed: bool

    if isinstance(a, bool) and guard_int(a):
        # TODO: Should be `int`
        reveal_type(a)  # revealed: bool
```
