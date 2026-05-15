# Constructor

When classes are instantiated, Python calls the metaclass's `__call__` method. The metaclass of most
Python classes is the class `builtins.type`.

`type.__call__` calls the `__new__` method of the class, which is responsible for creating the
instance. `__init__` is then called on the constructed instance with the same arguments that were
passed to `__new__`.

Both `__new__` and `__init__` are looked up using the descriptor protocol, i.e., `__get__` is called
if these attributes are descriptors. `__new__` is always treated as a static method, i.e., `cls` is
passed as the first argument. `__init__` has no special handling; it is fetched as a bound method
and called just like any other dunder method.

`type.__call__` does other things too, but this is not yet handled by us.

Since every class has `object` in its MRO, the default implementations are `object.__new__` and
`object.__init__`. They have some special behavior, namely:

- If neither `__new__` nor `__init__` are defined anywhere in the MRO of class (except for
    `object`), no arguments are accepted and `TypeError` is raised if any are passed.
- If `__new__` is defined but `__init__` is not, `object.__init__` will allow arbitrary arguments!

## Creating an instance of the `object` class itself

Test the behavior of the `object` class itself. As implementation has to ignore `object` own methods
as defined in typeshed due to behavior not expressible in typeshed (see above how `__init__` behaves
differently depending on whether `__new__` is defined or not), we have to test the behavior of
`object` itself.

```py
reveal_type(object())  # revealed: object

# error: [too-many-positional-arguments] "Too many positional arguments to class `object`: expected 0, got 1"
reveal_type(object(1))  # revealed: object
```

## No init or new

```py
class Foo: ...

reveal_type(Foo())  # revealed: Foo

# error: [too-many-positional-arguments] "Too many positional arguments to `object.__init__`: expected 1, got 2"
reveal_type(Foo(1))  # revealed: Foo
```

## `__new__` present on the class itself

```py
class Foo:
    def __new__(cls, x: int) -> "Foo":
        return object.__new__(cls)

reveal_type(Foo(1))  # revealed: Foo

# error: [invalid-argument-type] "Argument to constructor `Foo.__new__` is incorrect: Expected `int`, found `Literal["x"]`"
reveal_type(Foo("x"))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of constructor `Foo.__new__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to constructor `Foo.__new__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## `__new__` with an invalid decorator and unresolved return annotation

Regression test for <https://github.com/astral-sh/ty/issues/3470>.

```toml
[environment]
python-version = "3.14"
```

```py
from typing import TypeVar
from typing_extensions import deprecated

class C:
    @deprecated  # error: [invalid-argument-type] "LiteralString"
    def __new__() -> T:  # error: [empty-body] "TypeVar"
        pass

C()
T = TypeVar
```

## `__new__` present on a superclass

If the `__new__` method is defined on a superclass, we can still infer the signature of the
constructor from it.

```py
from typing_extensions import Self

class Base:
    def __new__(cls, x: int) -> Self:
        return cls(x)

class Foo(Base): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of constructor `Base.__new__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to constructor `Base.__new__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## `__new__` present but `__init__` missing

`object.__init__` allows arbitrary arguments when a custom `__new__` exists. This should not trigger
`__init__` argument errors.

```py
class Foo:
    def __new__(cls, x: int):
        return object.__new__(cls)

reveal_type(Foo(1))  # revealed: Foo

Foo(1)
# error: [too-many-positional-arguments] "Too many positional arguments to constructor `Foo.__new__`: expected 2, got 3"
Foo(1, 2)
```

## Conditional `__new__`

```py
def _(flag: bool) -> None:
    class Foo:
        if flag:
            def __new__(cls, x: int): ...

        else:
            def __new__(cls, x: int, y: int = 1): ...

    reveal_type(Foo(1))  # revealed: Foo
    # error: [invalid-argument-type] "Argument to constructor `Foo.__new__` is incorrect: Expected `int`, found `Literal["1"]`"
    # error: [invalid-argument-type] "Argument to constructor `Foo.__new__` is incorrect: Expected `int`, found `Literal["1"]`"
    reveal_type(Foo("1"))  # revealed: Foo
    # error: [missing-argument] "No argument provided for required parameter `x` of constructor `Foo.__new__`"
    # error: [missing-argument] "No argument provided for required parameter `x` of constructor `Foo.__new__`"
    reveal_type(Foo())  # revealed: Foo
    # error: [too-many-positional-arguments] "Too many positional arguments to constructor `Foo.__new__`: expected 2, got 3"
    reveal_type(Foo(1, 2))  # revealed: Foo
```

## A descriptor in place of `__new__`

```py
class SomeCallable:
    def __call__(self, cls, x: int) -> "Foo":
        obj = object.__new__(cls)
        obj.x = x
        return obj

class Descriptor:
    def __get__(self, instance, owner) -> SomeCallable:
        return SomeCallable()

class Foo:
    __new__: Descriptor = Descriptor()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `SomeCallable.__call__`"
reveal_type(Foo())  # revealed: Foo
```

## `__new__` is implicitly a static method, but explicitly marking it as one is harmless

```py
class Foo:
    @staticmethod
    def __new__(cls, x: int):
        return object.__new__(cls)

reveal_type(Foo(1))  # revealed: Foo
```

## `__new__` defined as a classmethod

Marking it as a classmethod, on the other hand, breaks at runtime.

```py
class Foo:
    @classmethod
    def __new__(cls, x: int):
        return object.__new__(cls)

# error: [invalid-argument-type] "Argument to bound method `Foo.__new__` is incorrect: Expected `int`, found `<class 'Foo'>`"
# error: [too-many-positional-arguments] "Too many positional arguments to bound method `Foo.__new__`: expected 1, got 2"
Foo(1)
```

## A callable instance in place of `__new__`

### Bound

```py
class Callable:
    def __call__(self, cls, x: int) -> "Foo":
        return object.__new__(cls)

class Foo:
    __new__ = Callable()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `Callable.__call__`"
reveal_type(Foo())  # revealed: Foo
```

### Possibly Unbound

#### Possibly unbound `__new__` method

```py
def _(flag: bool) -> None:
    class Foo:
        if flag:
            def __new__(cls):
                return object.__new__(cls)

    # error: [possibly-missing-implicit-call]
    reveal_type(Foo())  # revealed: Foo

    # error: [possibly-missing-implicit-call]
    # error: [too-many-positional-arguments]
    reveal_type(Foo(1))  # revealed: Foo
```

#### Possibly missing `__call__` on `__new__` callable

```py
def _(flag: bool) -> None:
    class Callable:
        if flag:
            def __call__(self, cls, x: int) -> "Foo":
                return object.__new__(cls)

    class Foo:
        __new__ = Callable()

    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly missing `__call__` method)"
    reveal_type(Foo(1))  # revealed: Foo
    # TODO should be - error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
    # but we currently infer the signature of `__call__` as unknown, so it accepts any arguments
    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly missing `__call__` method)"
    reveal_type(Foo())  # revealed: Foo
```

## `__init__` present on the class itself

If the class has an `__init__` method, we can infer the signature of the constructor from it.

```py
class Foo:
    def __init__(self, x: int): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of `Foo.__init__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to `Foo.__init__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
```

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

### `__new__` returning an intersection type

```py
from __future__ import annotations
from ty_extensions import Intersection

class Mixin:
    pass

class A:
    def __new__(cls) -> Intersection[A, Mixin]:
        raise NotImplementedError()

    def __init__(self, x: int) -> None: ...

# error: [missing-argument]
reveal_type(A())  # revealed: A & Mixin
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

### `__new__` returning a non-self typevar

When `__new__` returns a type variable that is not `Self`, we should specialize it before
categorizing the return type as instance or non-instance.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    def __new__(cls, x: T) -> T:
        return x

    def __init__(self) -> None: ...

# `Literal[1]` is not an instance of `C`, so `__init__` is skipped.
reveal_type(C(1))  # revealed: Literal[1]

def _(c: C[str]):
    # `C[str]` is an instance of `C`, so `__init__` is checked and fails.
    # error: [too-many-positional-arguments]
    reveal_type(C(c))  # revealed: C[str]
```

### Self-like `__new__` typevars should still provide `__init__` type context

When `__new__` returns the constructed type via a `cls: type[T] -> T` annotation, we should still
use `__init__` to provide argument type context for constructor arguments.

#### `Any`-typed `__new__` parameter should not block `__init__` type context

```py
from __future__ import annotations
from typing import Any, Callable, TypeVar

T = TypeVar("T", bound="SpanData")

class SpanData:
    def __new__(
        cls: type[T],
        name: str,
        on_finish: Any | None = None,
    ) -> T:
        return object.__new__(cls)

class Span(SpanData):
    def __init__(self, name: str, on_finish: list[Callable[[Span], None]] | None = None) -> None:
        pass

class Tracer:
    def _on_span_finish(self, span: Span) -> None:
        pass

    def start(self) -> None:
        Span("x", on_finish=[self._on_span_finish])
```

#### `object`-typed `__new__` parameter should not block `__init__` type context

```py
from typing import Callable, TypeVar

T = TypeVar("T", bound="SpanData")

class SpanData:
    def __new__(
        cls: type[T],
        name: str,
        on_finish: object | None = None,
    ) -> T:
        return object.__new__(cls)

class Span(SpanData):
    def __init__(self, name: str, on_finish: list[Callable[["Span"], None]] | None = None) -> None:
        pass

class Tracer:
    def _on_span_finish(self, span: "Span") -> None:
        pass

    def start(self) -> None:
        Span("x", on_finish=[self._on_span_finish])
```

#### `cls: type[T] -> T` should still allow literal promotion for invariant class type parameters

```py
from typing import Generic, TypeVar

S = TypeVar("S")
T = TypeVar("T", bound="Box")

class Box(Generic[S]):
    def __new__(cls: type[T], x: S) -> T:
        return super().__new__(cls)

reveal_type(Box(42))  # revealed: Box[int]
```

#### `typing.Self` return should still provide `__init__` type context

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations
from typing import Callable, Self, Any

class SpanData:
    def __new__(
        cls,
        name: str,
        on_finish: Any | None = None,
    ) -> Self:
        return object.__new__(cls)

class Span(SpanData):
    def __init__(self, name: str, on_finish: list[Callable[[Span], None]] | None = None) -> None:
        pass

class Tracer:
    def _on_span_finish(self, span: Span) -> None:
        pass

    def start(self) -> None:
        Span("x", on_finish=[self._on_span_finish])
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

### `__new__` can remap an explicit generic specialization

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Class8(Generic[T]):
    def __new__(cls, *args, **kwargs) -> "Class8[list[T]]":
        raise NotImplementedError

reveal_type(Class8[int]())  # revealed: Class8[list[int]]
reveal_type(Class8[str]())  # revealed: Class8[list[str]]
```

### `__new__` returning `Self` preserves explicit specialization

```py
from typing import Generic, TypeVar
from typing_extensions import Self

T = TypeVar("T")

class Class9(Generic[T]):
    def __new__(cls, x: T) -> Self:
        return super().__new__(cls)

reveal_type(Class9[int](1))  # revealed: Class9[int]
```

### `__new__` can fix generic specialization and still validate `__init__`

```toml
[environment]
python-version = "3.12"
```

```py
class C[T]:
    def __new__(cls) -> "C[int]":
        raise NotImplementedError()

    def __init__(self, x: int) -> None:
        pass

# error: [missing-argument]
reveal_type(C())  # revealed: C[int]
# error: [missing-argument]
reveal_type(C[str]())  # revealed: C[int]
# error: [missing-argument]
reveal_type(C[int]())  # revealed: C[int]
```

### `__new__` with method-level type variables mapping to class specialization

When `__new__` has its own type parameters that map to the class's type parameter through the return
type, we should correctly infer the class specialization.

```toml
[environment]
python-version = "3.12"
```

```py
class C[T]:
    x: T

    def __new__[S](cls, x: S) -> "C[tuple[S, S]]":
        return object.__new__(cls)

reveal_type(C(1))  # revealed: C[tuple[int, int]]
reveal_type(C("hello"))  # revealed: C[tuple[str, str]]
```

### `__new__` with arbitrary generic return types

When `__new__` has method-level type variables in the return type that don't map to the class's type
parameters, the resolved return type should be used directly.

```toml
[environment]
python-version = "3.12"
```

```py
class C:
    def __new__[S](cls, x: S) -> S:
        return x

reveal_type(C("foo"))  # revealed: Literal["foo"]
reveal_type(C(1))  # revealed: Literal[1]
```

### `__new__` returning non-instance generic containers

```toml
[environment]
python-version = "3.12"
```

```py
class C:
    def __new__[S](cls, x: S) -> list[S]:
        return [x]

reveal_type(C("foo"))  # revealed: list[str]
reveal_type(C(1))  # revealed: list[int]
```

### Failed `__new__` call with unambiguous non-instance return type

```py
class C:
    def __new__(cls, x: int) -> str:
        return str(x)

# error: [invalid-argument-type]
reveal_type(C("foo"))  # revealed: str
```

### Overloaded `__new__` with generic return types

Overloaded `__new__` methods should correctly resolve to the matching overload and infer the class
specialization from the overload's return type.

```py
from typing import Generic, Iterable, TypeVar, overload

T = TypeVar("T")
T1 = TypeVar("T1")
T2 = TypeVar("T2")

class MyZip(Generic[T]):
    @overload
    def __new__(cls) -> "MyZip[object]": ...
    @overload
    def __new__(cls, iter1: Iterable[T1], iter2: Iterable[T2]) -> "MyZip[tuple[T1, T2]]": ...
    def __new__(cls, *args, **kwargs) -> "MyZip[object]":
        raise NotImplementedError

def check(a: tuple[int, ...], b: tuple[str, ...]) -> None:
    reveal_type(MyZip(a, b))  # revealed: MyZip[tuple[int, str]]
    reveal_type(MyZip())  # revealed: MyZip[object]
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

The same mechanism should also hold for a `Self`-returning overload:

```py
from typing import overload
from typing_extensions import Self

class SimpleMixed:
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: str) -> Self: ...
    def __new__(cls, x: int | str) -> object: ...
    def __init__(self, x: str) -> None: ...

reveal_type(SimpleMixed(1))  # revealed: int
reveal_type(SimpleMixed("foo"))  # revealed: SimpleMixed
```

### Multiple matching `__new__` overloads

If overload resolution for `__new__` falls back to `Unknown` because the argument is `Any` or
`Unknown`, we should still validate downstream constructors:

```py
from typing import Any, overload
from typing_extensions import Self
from missing import Unknown  # type: ignore

class AmbiguousMixed:
    @overload
    def __new__(cls, x: int) -> Self: ...
    @overload
    def __new__(cls, x: str) -> str: ...
    def __new__(cls, x: int | str) -> Self | str:
        raise NotImplementedError

    def __init__(self) -> None: ...

def _(a: Any, u: Unknown):
    # error: [too-many-positional-arguments]
    reveal_type(AmbiguousMixed(a))  # revealed: Unknown

    # error: [too-many-positional-arguments]
    reveal_type(AmbiguousMixed(u))  # revealed: Unknown
```

### Mixed `__new__` overloads should not become declaration-order dependent

Reversing the declaration order of the same mixed overload set should not change the result when
overload resolution falls back to `Unknown`.

```py
from typing import Any, overload
from typing_extensions import Self
from missing import Unknown  # type: ignore

class ReverseAmbiguousMixed:
    @overload
    def __new__(cls, x: str) -> str: ...
    @overload
    def __new__(cls, x: int) -> Self: ...
    def __new__(cls, x: int | str) -> object:
        raise NotImplementedError

    def __init__(self) -> None: ...

def _(a: Any, u: Unknown):
    # error: [too-many-positional-arguments]
    reveal_type(ReverseAmbiguousMixed(a))  # revealed: Unknown

    # error: [too-many-positional-arguments]
    reveal_type(ReverseAmbiguousMixed(u))  # revealed: Unknown
```

### Overloaded non-instance `__new__` should preserve matched return type

When all `__new__` overloads return non-instance types, constructor return typing should still use
the matched overload's return type at each call site.

```py
from typing import overload

class F:
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: str) -> str: ...
    def __new__(cls, x: int | str) -> object: ...

reveal_type(F(1))  # revealed: int
reveal_type(F("foo"))  # revealed: str
```

### Invalid overloaded non-instance `__new__` should not invent an instance return

If no overload matches, we should report `Unknown` rather than falling back to the class instance
type.

```py
from typing import overload

class OnlyNonInstance:
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: str) -> str: ...
    def __new__(cls, x: int | str) -> object:
        raise NotImplementedError

# error: [no-matching-overload]
reveal_type(OnlyNonInstance(1.2))  # revealed: Unknown
```

### Mixed generic `__new__` overloads should still validate `__init__`

For generic classes, if an instance-returning `__new__` overload matches, we still need to validate
`__init__` even when another overload returns a non-instance type.

```py
from typing import Generic, TypeVar, overload
from typing_extensions import Self

T = TypeVar("T")

class E(Generic[T]):
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: T) -> Self: ...
    def __new__(cls, x: object) -> object: ...
    def __init__(self, x: T, y: str) -> None: ...

# The `T -> Self` overload is instance-returning, so `__init__` must also be checked.
# error: [missing-argument]
reveal_type(E("foo"))  # revealed: E[str]
```

### Mixed overloaded `__new__` should also normalize `cls: type[T] -> T` returns

The same selected-overload path should treat self-like `TypeVar` returns as instance-returning.

```py
from __future__ import annotations
from typing import Generic, TypeVar, overload

S = TypeVar("S")
T = TypeVar("T", bound="E")

class E(Generic[S]):
    @overload
    def __new__(cls, x: int, y: int) -> int: ...
    @overload
    def __new__(cls: type[T], x: S) -> T: ...
    def __new__(cls, *args: object) -> object: ...
    def __init__(self, x: S, y: str) -> None: ...

# The `type[T] -> T` overload is instance-returning, so `__init__` must also be checked.
# error: [missing-argument]
reveal_type(E("foo"))  # revealed: E[str]
reveal_type(E(1, 2))  # revealed: int
```

### Mixed overloaded `__new__` should preserve constructor literal promotion

When mixed `__new__` overloads defer `__init__` validation, the inferred constructor specialization
should still apply literal promotion from `__init__`.

```py
from typing import Generic, TypeVar, overload
from typing_extensions import Self

T = TypeVar("T")

class E(Generic[T]):
    @overload
    def __new__(cls, tag: int, y: object) -> int: ...
    @overload
    def __new__(cls, tag: str, y: object) -> Self: ...
    def __new__(cls, tag: int | str, y: object) -> object: ...
    def __init__(self, tag: str, y: T) -> None: ...

reveal_type(E("ok", 1))  # revealed: E[int]
reveal_type(E(1, 1))  # revealed: int
```

### Union of mixed constructors should preserve deferred `__init__` checks

```py
from typing import overload
from typing_extensions import Self

class C:
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: str) -> Self: ...
    def __new__(cls, x: int | str) -> object: ...
    def __init__(self, x: str, y: str) -> None: ...

class D:
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: str) -> Self: ...
    def __new__(cls, x: int | str) -> object: ...
    def __init__(self, x: str) -> None: ...

def f(flag: bool) -> None:
    ctor = C if flag else D

    # `str -> Self` is selected on both constructor branches. `C.__init__` still
    # requires `y`, so this should fail even after unioning constructor bindings.
    # error: [missing-argument]
    ctor("foo")
```

### Intersection of mixed constructors should discard failing deferred `__init__` checks

```py
from typing import overload

from ty_extensions import Intersection
from typing_extensions import Self

class C:
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: str) -> Self: ...
    def __new__(cls, x: int | str) -> object: ...
    def __init__(self, x: str, y: str) -> None: ...

class D:
    def __init__(self, x: str) -> None: ...

def f(ctor: Intersection[type[C], type[D]]) -> None:
    # `C.__new__` selects `str -> Self`, but `C.__init__` still rejects the call
    # because `y` is missing. `D` accepts the call, so the intersection should
    # succeed using only the `D` branch.
    reveal_type(ctor("foo"))  # revealed: D
```

### Union of generic constructor types with `__new__` should preserve specialization

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class E(Generic[T]):
    def __new__(cls, x: object):
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

class F(Generic[T]):
    def __new__(cls, x: object):
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

def f(flag: bool) -> None:
    ctor: type[E[int]] | type[F[int]]
    if flag:
        ctor = E
    else:
        ctor = F

    reveal_type(ctor(1))  # revealed: E[int] | F[int]
```

### Intersection of generic constructor types with `__new__` should preserve specialization

```py
from typing import Generic, TypeVar

from ty_extensions import Intersection

T = TypeVar("T")

class E(Generic[T]):
    def __new__(cls, x: object):
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

class F(Generic[T]):
    def __new__(cls, x: object):
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

def f(ctor: Intersection[type[E[int]], type[F[int]]]) -> None:
    reveal_type(ctor(1))  # revealed: E[int] & F[int]
```

### `__new__` returning a strict subclass preserves that return type

```py
class C:
    def __new__(cls) -> "D":
        return object.__new__(D)

class D(C): ...

# Preserve explicit strict-subclass constructor returns.
reveal_type(C())  # revealed: D
```

### Generic `__new__` returning a strict subclass preserves that return type

```toml
[environment]
python-version = "3.12"
```

```py
class C[T]:
    def __new__(cls, x: T) -> "D":
        raise NotImplementedError

    def __init__(self, x: object) -> None: ...

    x: T

class D(C[int]): ...

reveal_type(C("foo"))  # revealed: D
```

### Generic `__new__` subtype return should inherit specialization from `__init__`

```toml
[environment]
python-version = "3.12"
```

```py
class C[T]:
    def __new__(cls, x: object) -> "D[T]":
        raise NotImplementedError

    def __init__(self, x: T) -> None: ...

    x: T

class D[T](C[T]): ...

reveal_type(C("foo"))  # revealed: D[str]
```

### Mixed overloaded `__new__` preserving strict-subclass return

```py
from typing import overload

class Base:
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: str) -> "Child": ...
    def __new__(cls, x: int | str) -> object: ...
    def __init__(self, x: str) -> None: ...

class Child(Base): ...

reveal_type(Base(1))  # revealed: int
reveal_type(Base("foo"))  # revealed: Child
```

## Generic constructor inference

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Box(Generic[T]):
    def __init__(self, x: T) -> None: ...

reveal_type(Box(1))  # revealed: Box[int]
```

## Generic constructor inference from overloaded `__init__` self types

```py
from __future__ import annotations

from typing import Generic, TypeVar, overload

T = TypeVar("T")
CT = TypeVar("CT")

class ClassSelector(Generic[T]):
    @overload
    def __init__(
        self: ClassSelector[CT],
        *,
        default: CT,
        class_: type[CT],
    ) -> None: ...
    @overload
    def __init__(
        self: ClassSelector[CT | None],
        *,
        default: None = None,
        class_: type[CT],
    ) -> None: ...
    def __init__(self, *, default=None, class_=None): ...

class MyClass:
    pass

a = ClassSelector(default=MyClass(), class_=MyClass)
reveal_type(a)  # revealed: ClassSelector[MyClass]

b = ClassSelector(class_=MyClass)
reveal_type(b)  # revealed: ClassSelector[MyClass | None]

# Explicit constructor specializations still reject incompatible inferred `self` types.
ClassSelector[int](class_=MyClass)  # error: [invalid-argument-type]

class RequiredClassSelector(Generic[T]):
    def __init__(self: RequiredClassSelector[CT | None], *, class_: type[CT]) -> None: ...

reveal_type(RequiredClassSelector(class_=MyClass))  # revealed: RequiredClassSelector[MyClass | None]
```

## `__init__` can remap constructor generic arguments via `self` annotation

```py
from typing import Generic, TypeVar

T1 = TypeVar("T1")
T2 = TypeVar("T2")

V1 = TypeVar("V1")
V2 = TypeVar("V2")

class Class6(Generic[T1, T2]):
    def __init__(self: "Class6[V1, V2]", value1: V1, value2: V2) -> None: ...

reveal_type(Class6(0, ""))  # revealed: Class6[int, str]
reveal_type(Class6[int, str](0, ""))  # revealed: Class6[int, str]

class Class7(Generic[T1, T2]):
    def __init__(self: "Class7[V2, V1]", value1: V1, value2: V2) -> None: ...

reveal_type(Class7(0, ""))  # revealed: Class7[str, int]
reveal_type(Class7[str, int](0, ""))  # revealed: Class7[str, int]
```

## Constructor calls through `type[T]` with a bound TypeVar

```py
from typing import TypeVar

class C:
    def __new__(cls, x: int, y: str): ...

T = TypeVar("T", bound=C)

def f(cls: type[T]):
    # error: [missing-argument] "No argument provided for required parameter `y` of constructor `C.__new__`"
    cls(1)
    # error: [invalid-argument-type] "Argument to constructor `C.__new__` is incorrect: Expected `str`, found `Literal[2]`"
    cls(1, 2)
    reveal_type(cls(1, "foo"))  # revealed: T@f
```

## Union of constructors

```py
class A:
    def __init__(self, x: int) -> None:
        self.x = x

class B:
    def __init__(self, x: int) -> None:
        self.x = x

def f(flag: bool):
    cls = A if flag else B
    reveal_type(cls(1))  # revealed: A | B
```

## `__init__` present on a superclass

If the `__init__` method is defined on a superclass, we can still infer the signature of the
constructor from it.

```py
class Base:
    def __init__(self, x: int): ...

class Foo(Base): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of `Base.__init__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to `Base.__init__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## Conditional `__init__`

```py
def _(flag: bool) -> None:
    class Foo:
        if flag:
            def __init__(self, x: int): ...

        else:
            def __init__(self, x: int, y: int = 1): ...

    reveal_type(Foo(1))  # revealed: Foo
    # error: [invalid-argument-type] "Argument to `Foo.__init__` is incorrect: Expected `int`, found `Literal["1"]`"
    # error: [invalid-argument-type] "Argument to `Foo.__init__` is incorrect: Expected `int`, found `Literal["1"]`"
    reveal_type(Foo("1"))  # revealed: Foo
    # error: [missing-argument] "No argument provided for required parameter `x` of `Foo.__init__`"
    # error: [missing-argument] "No argument provided for required parameter `x` of `Foo.__init__`"
    reveal_type(Foo())  # revealed: Foo
    # error: [too-many-positional-arguments] "Too many positional arguments to `Foo.__init__`: expected 2, got 3"
    reveal_type(Foo(1, 2))  # revealed: Foo
```

## A descriptor in place of `__init__`

```py
class SomeCallable:
    # TODO: at runtime `__init__` is checked to return `None` and
    # a `TypeError` is raised if it doesn't. However, apparently
    # this is not true when the descriptor is used as `__init__`.
    # However, we may still want to check this.
    def __call__(self, x: int) -> str:
        return "a"

class Descriptor:
    def __get__(self, instance, owner) -> SomeCallable:
        return SomeCallable()

class Foo:
    __init__: Descriptor = Descriptor()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `SomeCallable.__call__`"
reveal_type(Foo())  # revealed: Foo
```

## A callable instance in place of `__init__`

### Bound

```py
class Callable:
    def __call__(self, x: int) -> None:
        pass

class Foo:
    __init__ = Callable()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `Callable.__call__`"
reveal_type(Foo())  # revealed: Foo
```

### Possibly Unbound

```py
def _(flag: bool) -> None:
    class Callable:
        if flag:
            def __call__(self, x: int) -> None:
                pass

    class Foo:
        __init__ = Callable()

    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly missing `__call__` method)"
    reveal_type(Foo(1))  # revealed: Foo
    # TODO should be - error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
    # but we currently infer the signature of `__call__` as unknown, so it accepts any arguments
    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly missing `__call__` method)"
    reveal_type(Foo())  # revealed: Foo
```

## `__new__` and `__init__` both present

### Compatible signatures

But they can also be compatible, but not identical. We should correctly report errors only for the
mthod that would fail.

```py
class Foo:
    def __new__(cls, *args, **kwargs):
        return object.__new__(cls)

    def __init__(self, x: int) -> None:
        self.x = x

# error: [missing-argument] "No argument provided for required parameter `x` of `Foo.__init__`"
reveal_type(Foo())  # revealed: Foo
reveal_type(Foo(1))  # revealed: Foo

# error: [too-many-positional-arguments] "Too many positional arguments to `Foo.__init__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
```

### Conflicting parameter types

```py
class Foo:
    def __new__(cls, x: int):
        return object.__new__(cls)

    def __init__(self, x: str) -> None:
        self.x = x

# error: [invalid-argument-type] "Argument to `Foo.__init__` is incorrect: Expected `str`, found `Literal[1]`"
Foo(1)

# error: [invalid-argument-type] "Argument to constructor `Foo.__new__` is incorrect: Expected `int`, found `Literal["x"]`"
Foo("x")
```

### Incompatible signatures

```py
import abc

class Foo:
    def __new__(cls) -> "Foo":
        return object.__new__(cls)

    def __init__(self, x):
        self.x = 42

# error: [missing-argument] "No argument provided for required parameter `x` of `Foo.__init__`"
reveal_type(Foo())  # revealed: Foo

# error: [too-many-positional-arguments] "Too many positional arguments to constructor `Foo.__new__`: expected 1, got 2"
reveal_type(Foo(42))  # revealed: Foo

class Foo2:
    def __new__(cls, x) -> "Foo2":
        return object.__new__(cls)

    def __init__(self):
        pass

# error: [missing-argument] "No argument provided for required parameter `x` of constructor `Foo2.__new__`"
reveal_type(Foo2())  # revealed: Foo2

# error: [too-many-positional-arguments] "Too many positional arguments to `Foo2.__init__`: expected 1, got 2"
reveal_type(Foo2(42))  # revealed: Foo2

class Foo3(metaclass=abc.ABCMeta):
    def __new__(cls) -> "Foo3":
        return object.__new__(cls)

    def __init__(self, x):
        self.x = 42

# error: [missing-argument] "No argument provided for required parameter `x` of `Foo3.__init__`"
reveal_type(Foo3())  # revealed: Foo3

# error: [too-many-positional-arguments] "Too many positional arguments to constructor `Foo3.__new__`: expected 1, got 2"
reveal_type(Foo3(42))  # revealed: Foo3

class Foo4(metaclass=abc.ABCMeta):
    def __new__(cls, x) -> "Foo4":
        return object.__new__(cls)

    def __init__(self):
        pass

# error: [missing-argument] "No argument provided for required parameter `x` of constructor `Foo4.__new__`"
reveal_type(Foo4())  # revealed: Foo4

# error: [too-many-positional-arguments] "Too many positional arguments to `Foo4.__init__`: expected 1, got 2"
reveal_type(Foo4(42))  # revealed: Foo4
```

### Lookup of `__new__`

The `__new__` method is always invoked on the class itself, never on the metaclass. This is
different from how other dunder methods like `__lt__` are implicitly called (always on the
meta-type, never on the type itself).

```py
from typing_extensions import Literal

class Meta(type):
    def __new__(mcls, name, bases, namespace, /, **kwargs):
        return super().__new__(mcls, name, bases, namespace)

    def __lt__(cls, other) -> Literal[True]:
        return True

class C(metaclass=Meta): ...

# No error is raised here, since we don't implicitly call `Meta.__new__`
reveal_type(C())  # revealed: C

# Meta.__lt__ is implicitly called here:
reveal_type(C < C)  # revealed: Literal[True]
```
