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
    c: TypeGuard[bool],
    d: TypeIs[tuple[TypeOf[bytes]]],
    e: TypeGuard,  # error: [invalid-type-form]
    f: TypeIs,  # error: [invalid-type-form]
):
    reveal_type(a)  # revealed: TypeGuard[str]
    reveal_type(b)  # revealed: TypeIs[str | int]
    reveal_type(c)  # revealed: TypeGuard[bool]
    reveal_type(d)  # revealed: TypeIs[tuple[<class 'bytes'>]]
    reveal_type(e)  # revealed: Unknown
    reveal_type(f)  # revealed: Unknown

# error: [invalid-return-type] "Function always implicitly returns `None`, which is not assignable to return type `TypeGuard[str]`"
def _(a) -> TypeGuard[str]: ...

# error: [invalid-return-type] "Function always implicitly returns `None`, which is not assignable to return type `TypeIs[str]`"
def _(a) -> TypeIs[str]: ...
def f(a) -> TypeGuard[str]:
    return True

def g(a) -> TypeIs[str]:
    return True

def _(a: object):
    reveal_type(f(a))  # revealed: TypeGuard[str @ a]
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

## Methods

Methods narrow the first positional argument after `self` or `cls`

```py
from typing import TypeGuard

class C:
    def f(self, x: object) -> TypeGuard[str]:
        return True

    @classmethod
    def g(cls, x: object) -> TypeGuard[int]:
        return True
    # TODO: this could error at definition time
    def h(self) -> TypeGuard[str]:
        return True
    # TODO: this could error at definition time
    @classmethod
    def j(cls) -> TypeGuard[int]:
        return True

def _(x: object):
    if C().f(x):
        reveal_type(x)  # revealed: str
    if C.f(C(), x):
        # TODO: should be str
        reveal_type(x)  # revealed: object
    if C.g(x):
        reveal_type(x)  # revealed: int
    if C().g(x):
        reveal_type(x)  # revealed: int
    if C().h():  # error: [invalid-type-guard-call] "Type guard call does not have a target"
        pass
    if C.j():  # error: [invalid-type-guard-call] "Type guard call does not have a target"
        pass
```

```py
from typing_extensions import TypeIs

def is_int(val: object) -> TypeIs[int]:
    return isinstance(val, int)

class A:
    def is_int(self, val: object) -> TypeIs[int]:
        return isinstance(val, int)

    @classmethod
    def is_int2(cls, val: object) -> TypeIs[int]:
        return isinstance(val, int)

def _(x: object):
    if is_int(x):
        reveal_type(x)  # revealed: int

    if A().is_int(x):
        reveal_type(x)  # revealed: int

    if A().is_int2(x):
        reveal_type(x)  # revealed: int

    if A.is_int2(x):
        reveal_type(x)  # revealed: int
```

## Arguments to special forms

`TypeGuard` and `TypeIs` accept exactly one type argument.

```py
from typing_extensions import TypeGuard, TypeIs

a = 123

# error: [invalid-type-form] "Special form `typing.TypeGuard` expected exactly one type parameter"
def f(_) -> TypeGuard[int, str]: ...

# error: [invalid-type-form] "Special form `typing.TypeIs` expected exactly one type parameter"
# error: [invalid-type-form] "Variable of type `Literal[123]` is not allowed in a type expression"
def g(_) -> TypeIs[a, str]: ...

reveal_type(f(0))  # revealed: Unknown
reveal_type(g(0))  # revealed: Unknown
```

## Return types

All code paths in a type guard function must return booleans.

```py
from typing_extensions import Literal, TypeGuard, TypeIs, assert_never

def _(a: object, flag: bool) -> TypeGuard[str]:
    if flag:
        # error: [invalid-return-type] "Return type does not match returned value: expected `TypeGuard[str]`, found `Literal[0]`"
        return 0

    # error: [invalid-return-type] "Return type does not match returned value: expected `TypeGuard[str]`, found `Literal["foo"]`"
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

    if g(*d):
        ...

    if f("foo"):  # TODO: error: [invalid-type-guard-call]
        ...

    if g(a=d):  # error: [invalid-type-guard-call]
        ...
```

## Narrowing

```toml
[environment]
python-version = "3.12"
```

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
        reveal_type(a)  # revealed: Foo
    else:
        reveal_type(a)  # revealed: Foo | Bar

    if is_bar(a):
        reveal_type(a)  # revealed: Bar
    else:
        reveal_type(a)  # revealed: Foo & ~Bar
```

```py
from typing import TypeGuard, reveal_type

class P:
    pass

class A:
    pass

class B:
    pass

def is_b(val: object) -> TypeGuard[B]:
    return isinstance(val, B)

def _(x: P):
    if isinstance(x, A) or is_b(x):
        reveal_type(x)  # revealed: B | (P & A)
```

Attribute and subscript narrowing is supported:

```py
from typing_extensions import Any, Generic, Protocol, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    v: T

def _(a: tuple[Foo, Bar] | tuple[Bar, Foo], c: C[Any]):
    if reveal_type(guard_foo(a[1])):  # revealed: TypeGuard[Foo @ a[1]]
        reveal_type(a)  # revealed: tuple[Foo, Bar] | tuple[Bar, Foo]
        reveal_type(a[1])  # revealed: Foo

    if reveal_type(is_bar(a[0])):  # revealed: TypeIs[Bar @ a[0]]
        reveal_type(a)  # revealed: tuple[Foo, Bar] | tuple[Bar, Foo]
        reveal_type(a[0])  # revealed: Bar

    if reveal_type(guard_foo(c.v)):  # revealed: TypeGuard[Foo @ c.v]
        reveal_type(c)  # revealed: C[Any]
        reveal_type(c.v)  # revealed: Foo

    if reveal_type(is_bar(c.v)):  # revealed: TypeIs[Bar @ c.v]
        reveal_type(c)  # revealed: C[Any]
        reveal_type(c.v)  # revealed: Any & Bar
```

Indirect usage is supported within the same scope:

```py
def _(a: Foo | Bar):
    b = guard_foo(a)
    c = is_bar(a)

    reveal_type(a)  # revealed: Foo | Bar
    reveal_type(b)  # revealed: TypeGuard[Foo @ a]
    reveal_type(c)  # revealed: TypeIs[Bar @ a]

    if b:
        # TODO should be `Foo`
        reveal_type(a)  # revealed: Foo | Bar
    else:
        reveal_type(a)  # revealed: Foo | Bar

    if c:
        # TODO should be `Bar`
        reveal_type(a)  # revealed: Foo | Bar
    else:
        # TODO should be `Foo & ~Bar`
        reveal_type(a)  # revealed: Foo | Bar
```

Further writes to the narrowed place invalidate the narrowing:

```py
def _(x: Foo | Bar, flag: bool) -> None:
    b = is_bar(x)
    reveal_type(b)  # revealed: TypeIs[Bar @ x]

    if flag:
        x = Foo()

    if b:
        reveal_type(x)  # revealed: Foo | Bar
```

The `TypeIs` type remains effective across generic boundaries:

```py
from typing_extensions import TypeVar

T = TypeVar("T")

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

For generics, we transform the argument passed into `TypeIs[]` from `X` to `Top[X]`. This helps
especially when using various functions from typeshed that are annotated as returning
`TypeIs[SomeCovariantGeneric[Any]]` to avoid false positives in other type checkers. For ty's
purposes, it would usually lead to more intuitive results if `object` was used as the specialization
for a covariant generic inside the `TypeIs` special form, but this is mitigated by our implicit
transformation from `TypeIs[SomeCovariantGeneric[Any]]` to `TypeIs[Top[SomeCovariantGeneric[Any]]]`
(which just simplifies to `TypeIs[SomeCovariantGeneric[object]]`).

```py
class Unrelated: ...

class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

def is_instance_of_covariant(arg: object) -> TypeIs[Covariant[Any]]:
    return isinstance(arg, Covariant)

def needs_instance_of_unrelated(arg: Unrelated):
    pass

def _(x: Unrelated | Covariant[int]):
    if is_instance_of_covariant(x):
        raise RuntimeError("oh no")

    reveal_type(x)  # revealed: Unrelated & ~Covariant[object]

    # We would emit a false-positive diagnostic here if we didn't implicitly transform
    # `TypeIs[Covariant[Any]]` to `TypeIs[Covariant[object]]`
    needs_instance_of_unrelated(x)
```

## `TypeGuard` special cases

```py
from typing import Any
from typing_extensions import TypeGuard, TypeIs

class Foo: ...
class Bar: ...
class Baz(Bar): ...

def guard_foo(a: object) -> TypeGuard[Foo]:
    return True

def guard_bar(a: object) -> TypeGuard[Bar]:
    return True

def is_bar(a: object) -> TypeIs[Bar]:
    return True

def does_not_narrow_in_negative_case(a: Foo | Bar):
    if not guard_foo(a):
        reveal_type(a)  # revealed: Foo | Bar
    else:
        reveal_type(a)  # revealed: Foo

def narrowed_type_must_be_exact(a: object, b: Baz):
    if guard_foo(b):
        reveal_type(b)  # revealed: Foo

    if isinstance(a, Baz) and is_bar(a):
        reveal_type(a)  # revealed: Baz

    if isinstance(a, Bar) and guard_foo(a):
        reveal_type(a)  # revealed: Foo
        if guard_bar(a):
            reveal_type(a)  # revealed: Bar
```

## TypeGuard overrides normal constraints

TypeGuard constraints override any previous narrowing, but additional "regular" constraints can be
added on to TypeGuard constraints.

```py
from typing_extensions import TypeGuard, TypeIs

class A: ...
class B: ...
class C: ...

def f(x: object) -> TypeGuard[A]:
    return True

def g(x: object) -> TypeGuard[B]:
    return True

def h(x: object) -> TypeIs[C]:
    return True

def _(x: object):
    if f(x) and g(x) and h(x):
        reveal_type(x)  # revealed: B & C
```

## Boolean logic with TypeGuard and TypeIs

TypeGuard constraints need to properly distribute through boolean operations.

```py
from typing_extensions import TypeGuard, TypeIs

class A: ...
class B: ...
class C: ...

def f(x: object) -> TypeIs[A]:
    return True

def g(x: object) -> TypeGuard[B]:
    return True

def h(x: object) -> TypeIs[C]:
    return True

def _(x: object):
    # g(x) or h(x) should give B | C
    # Then f(x) and (...) should distribute: (f(x) and g(x)) or (f(x) and h(x))
    # Which is (Regular(A) & TypeGuard(B)) | (Regular(A) & Regular(C))
    # TypeGuard clobbers in the first branch, giving: B | (A & C)
    if f(x) and (g(x) or h(x)):
        reveal_type(x)  # revealed: B | (A & C)
```
