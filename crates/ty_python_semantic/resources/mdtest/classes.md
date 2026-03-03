# Class definitions

## `__new__` return type

Python's `__new__` method can return any type, not just an instance of the class. When `__new__`
returns a type that is not a subtype of the class instance type, we use the returned type directly,
without checking `__init__`.

### `__new__` returning a different type

```py
class ReturnsInt:
    def __new__(cls) -> int:
        return 42

reveal_type(ReturnsInt())  # revealed: int

x: int = ReturnsInt()  # OK
y: ReturnsInt = ReturnsInt()  # error: [invalid-assignment]
```

In this case, we don't validate `__init__`:

```py
class ReturnsIntWithInit:
    def __new__(cls) -> int:
        return 42

    def __init__(self, x: str) -> None: ...

# No error from missing argument to `__init__`:
reveal_type(ReturnsIntWithInit())  # revealed: int
```

### `__new__` returning a union type

```py
class MaybeInt:
    def __new__(cls, value: str) -> "int | MaybeInt":
        try:
            return int(value)
        except ValueError:
            return object.__new__(cls)

reveal_type(MaybeInt("42"))  # revealed: int | MaybeInt

a: int | MaybeInt = MaybeInt("42")  # OK
b: int = MaybeInt("42")  # error: [invalid-assignment]
```

### `__new__` returning the class type

When `__new__` returns the type of the instance being constructed, we use that type:

```py
class Normal:
    def __new__(cls) -> "Normal":
        return object.__new__(cls)

reveal_type(Normal())  # revealed: Normal
```

And we do validate `__init__`:

```py
class NormalWithInit:
    def __new__(cls) -> "NormalWithInit":
        return object.__new__(cls)

    def __init__(self, x: int) -> None: ...

# error: [missing-argument]
reveal_type(NormalWithInit())  # revealed: NormalWithInit
```

### `__new__` with no return type annotation

When `__new__` has no return type annotation, we fall back to the instance type.

```py
class NoAnnotation:
    def __new__(cls):
        return object.__new__(cls)

reveal_type(NoAnnotation())  # revealed: NoAnnotation
```

### `__new__` returning `Any`

Per the spec, "an explicit return type of `Any` should be treated as a type that is not an instance
of the class being constructed." This means `__init__` is not called and the return type is `Any`.

```py
from typing import Any

class ReturnsAny:
    def __new__(cls) -> Any:
        return 42

    def __init__(self, x: int) -> None:
        pass

# __init__ is skipped because `-> Any` is treated as non-instance per spec
reveal_type(ReturnsAny())  # revealed: Any
```

### `__new__` returning `Never`

When `__new__` returns `Never`, the call is terminal and `__init__` is skipped.

```py
from typing_extensions import Never

class NewNeverReturns:
    def __new__(cls) -> Never:
        raise NotImplementedError

    def __init__(self, x: int) -> None:
        pass

# `__init__` is skipped because `__new__` never returns.
reveal_type(NewNeverReturns())  # revealed: Never
```

### `__new__` returning a union containing `Any`

When `__new__` returns a union containing `Any`, since we don't consider `Any` a subtype of the
instance type, `__init__` is skipped.

```py
from typing import Any

class MaybeAny:
    def __new__(cls, value: int) -> "MaybeAny | Any":
        if value > 0:
            return object.__new__(cls)
        return None

    def __init__(self) -> None:
        pass

reveal_type(MaybeAny(1))  # revealed: MaybeAny | Any
```

### `__new__` returning a specific class affects subclasses

When `__new__` returns a specific class (e.g., `-> Foo`), this is an instance type for `Foo` itself,
so `__init__` is checked. But for a subclass `Bar(Foo)`, the return type `Foo` is NOT an instance of
`Bar`, so the `__new__` return type is used directly and `Bar.__init__` is skipped.

```py
class Foo:
    def __new__(cls, x: int = 0) -> "Foo":
        return object.__new__(cls)

    def __init__(self, x: int) -> None:
        pass

class Bar(Foo):
    def __init__(self, y: str) -> None:
        pass

# For Foo: return type `Foo` IS an instance of `Foo`, so `__init__` is checked.
Foo()  # error: [missing-argument]
reveal_type(Foo(1))  # revealed: Foo

# For Bar: return type `Foo` is NOT an instance of `Bar`, so `__init__` is
# skipped and `Foo` is used directly.
reveal_type(Bar())  # revealed: Foo
reveal_type(Bar(1))  # revealed: Foo
```

### Mixed `__new__` overloads

If some `__new__` overloads are instance-returning and some are not, the return type (and `__init__`
validation) are resolved correctly for each call site:

```py
from __future__ import annotations
from typing import Any, Literal, overload

class A: ...
class B: ...
class C: ...
class D: ...

class Test:
    @overload
    def __new__(cls, x: A) -> A: ...
    @overload
    def __new__(cls, x: str) -> Test: ...
    def __new__(cls, x: A | str) -> A | Test:
        raise NotImplementedError()

    def __init__(self, x: Literal["ok"]) -> None:
        pass

# `A` matches the first `__new__` overload, which returns `A`, bypassing `__init__` since `A` is
# not a subtype of `Test`.
reveal_type(Test(A()))  # revealed: A

# `str` returns `Test` from `__new__`, but `__init__` rejects `Literal["bad"]`.
# error: [invalid-argument-type]
reveal_type(Test("bad"))  # revealed: Test

# `Literal["ok"]` returns `Test` from `__new__`, and is accepted by `__init__`.
reveal_type(Test("ok"))  # revealed: Test
```

## Deferred resolution of bases

### Only the stringified name is deferred

If a class base contains a stringified name, only that name is deferred. Other names are resolved
normally.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import reveal_mro

A = int

class G[T]: ...
class C(A, G["B"]): ...

A = str
B = bytes

reveal_mro(C)  # revealed: (<class 'C'>, <class 'int'>, <class 'G[bytes]'>, typing.Generic, <class 'object'>)
```

## Starred bases

Fixed-length tuples are unpacked when used as starred base classes:

```py
from ty_extensions import reveal_mro

class A: ...
class B: ...
class C: ...

bases = (A, B, C)

class Foo(*bases): ...

# revealed: (<class 'Foo'>, <class 'A'>, <class 'B'>, <class 'C'>, <class 'object'>)
reveal_mro(Foo)
```

Variable-length tuples cannot be unpacked, so they fall back to `Unknown`:

```py
from ty_extensions import reveal_mro

def get_bases() -> tuple[type, ...]:
    return (int, str)

# error: [unsupported-base] "Unsupported class base"
class Bar(*get_bases()): ...

# revealed: (<class 'Bar'>, Unknown, <class 'object'>)
reveal_mro(Bar)
```
