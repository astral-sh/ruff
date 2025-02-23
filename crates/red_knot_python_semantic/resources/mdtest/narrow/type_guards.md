# User-defined type guards

User-defined type guards are functions of which the return type is either `TypeGuard[...]` or
`TypeIs[...]`.

## Display

```py
from knot_extensions import Intersection, Not, TypeOf
from typing_extensions import TypeGuard, TypeIs

def _(
    a: TypeGuard[str],
    b: TypeIs[str | int],
    c: TypeGuard[Intersection[complex, Not[int], Not[float]]],
    d: TypeIs[tuple[TypeOf[bytes]]],
):
    reveal_type(a)  # revealed: TypeGuard[str]
    reveal_type(b)  # revealed: TypeIs[str | int]
    reveal_type(c)  # revealed: TypeGuard[complex & ~int & ~float]
    reveal_type(d)  # revealed: TypeIs[tuple[Literal[bytes]]]

def f(a) -> TypeGuard[str]: ...
def g(a) -> TypeIs[str]: ...
def _(a: object):
    reveal_type(f(a))  # revealed: TypeGuard[a, str]
    reveal_type(g(a))  # revealed: TypeIs[a, str]
```

## Parameters

A user-defined type guard must accept at least one positional argument, (in addition to `self`/`cls`
for non-static methods).

```py
from typing_extensions import TypeGuard, TypeIs

# error: [invalid-type-guard-definition]
def _() -> TypeGuard[str]: ...

# error: [invalid-type-guard-definition]
def _(**kwargs) -> TypeIs[str]: ...

class _:
    # fine
    def _(self, /, a) -> TypeGuard[str]: ...
    @classmethod
    def _(cls, a) -> TypeGuard[str]: ...
    @staticmethod
    def _(a) -> TypeIs[str]: ...

    # errors
    def _(self) -> TypeGuard[str]: ...  # error: [invalid-type-guard-definition]
    def _(self, /, *, a) -> TypeGuard[str]: ...  # error: [invalid-type-guard-definition]
    @classmethod
    def _(cls) -> TypeIs[str]: ...  # error: [invalid-type-guard-definition]
    @classmethod
    def _() -> TypeIs[str]: ...  # error: [invalid-type-guard-definition]
    @staticmethod
    def _(*, a) -> TypeGuard[str]: ...  # error: [invalid-type-guard-definition]
```

For `TypeIs` functions, the narrowed type must be assignable to the declared type of that parameter,
if any.

```py
from typing import Any
from typing_extensions import TypeGuard, TypeIs

def _(a: object) -> TypeIs[str]: ...
def _(a: Any) -> TypeIs[str]: ...
def _(a: tuple[object]) -> TypeIs[tuple[str]]: ...
def _(a: str | Any) -> TypeIs[str]: ...
def _(a) -> TypeIs[str]: ...

# error: [invalid-type-guard-definition]
def _(a: int) -> TypeIs[str]: ...

# error: [invalid-type-guard-definition]
def _(a: bool | str) -> TypeIs[int]: ...
```

## Arguments to special forms

`TypeGuard` and `TypeIs` accept exactly one type argument.

```py
from typing_extensions import TypeGuard, TypeIs

a = 123

# error: [invalid-type-form]
def f(_) -> TypeGuard[int, str]: ...

# error: [invalid-type-form]
def g(_) -> TypeIs[a, str]: ...

reveal_type(f(0))  # revealed: Unknown
reveal_type(g(0))  # revealed: Unknown
```

## Return types

All code paths in a type guard function must return booleans.

```py
from typing_extensions import Literal, TypeGuard, TypeIs, assert_never

def f(a: object, flag: bool) -> TypeGuard[str]:
    if flag:
        # TODO: Emit a diagnostic
        return 1

    # TODO: Emit a diagnostic
    return ""

def g(a: Literal["foo", "bar"]) -> TypeIs[Literal["foo"]]:
    match a:
        case "foo":
            # Logically wrong, but allowed regardless
            return False
        case "bar":
            return False
        case _:
            assert_never(a)
```

## Invalid calls

```py
from typing import Any
from typing_extensions import TypeGuard, TypeIs

def f(a: object) -> TypeGuard[str]: ...
def g(a: object) -> TypeIs[int]: ...
def _(d: Any):
    if f():  # error: [missing-argument]
        ...

    # TODO: Is this error correct?
    if g(*d):  # error: [missing-argument]
        ...

    if f("foo"):  # error: [invalid-type-guard-call]
        ...

    if g(a=d):  # error: [invalid-type-guard-call]
        ...

def _(a: tuple[str, int] | tuple[int, str]):
    if g(a[0]):  # error: [invalid-type-guard-call]
        # TODO: Should be `tuple[str, int]`
        reveal_type(a)  # revealed: tuple[str, int] | tuple[int, str]
```

## Narrowing

```py
from typing import Any
from typing_extensions import TypeGuard, TypeIs

def guard_str(a: object) -> TypeGuard[str]: ...
def is_int(a: object) -> TypeIs[int]: ...
def _(a: str | int):
    if guard_str(a):
        reveal_type(a)  # revealed: str
    else:
        reveal_type(a)  # revealed: str | int

    if is_int(a):
        reveal_type(a)  # revealed: int
    else:
        reveal_type(a)  # revealed: str & ~int

def _(a: str | int):
    b = guard_str(a)
    c = is_int(a)

    reveal_type(a)  # revealed: str | int
    reveal_type(b)  # revealed: TypeGuard[a, str]
    reveal_type(c)  # revealed: TypeIs[a, int]

    if b:
        reveal_type(a)  # revealed: str
    else:
        reveal_type(a)  # revealed: str | int

    if c:
        reveal_type(a)  # revealed: int
    else:
        reveal_type(a)  # revealed: str

def _(x: str | int, flag: bool) -> None:
    b = is_int(x)
    reveal_type(b)  # revealed: TypeIs[x, int]

    if flag:
        x = ""

    if b:
        reveal_type(x)  # revealed: str | int
```

## `TypeGuard` special cases

```py
from typing import Any
from typing_extensions import TypeGuard

def guard_int(a: object) -> TypeGuard[int]: ...
def is_int(a: object) -> TypeGuard[int]: ...
def does_not_narrow_in_negative_case(a: str | int):
    if not guard_int(a):
        reveal_type(a)  # revealed: str | int
    else:
        reveal_type(a)  # revealed: int

def narrowed_type_must_be_exact(a: object, b: bool):
    if guard_int(b):
        reveal_type(b)  # revealed: int

    if isinstance(a, bool) and is_int(a):
        reveal_type(a)  # revealed: bool

    if isinstance(a, bool) and guard_int(a):
        reveal_type(a)  # revealed: int
```
