# Dunder calls

## Introduction

This test suite explains and documents how dunder methods are looked up and called. Throughout the
document, we use `__getitem__` as an example, but the same principles apply to other dunder methods.

Dunder methods are implicitly called when using certain syntax. For example, the index operator
`obj[key]` calls the `__getitem__` method under the hood. Exactly *how* a dunder method is looked up
and called works slightly different from regular methods. Dunder methods are not looked up on `obj`
directly, but rather on `type(obj)`. But in many ways, they still *act* as if they were called on
`obj` directly. If the `__getitem__` member of `type(obj)` is a descriptor, it is called with `obj`
as the `instance` argument to `__get__`. A desugared version of `obj[key]` is roughly equivalent to
`getitem_desugared(obj, key)` as defined below:

```py
from typing import Any

def find_name_in_mro(typ: type, name: str) -> Any:
    # See implementation in https://docs.python.org/3/howto/descriptor.html#invocation-from-an-instance
    pass

def getitem_desugared(obj: object, key: object) -> object:
    getitem_callable = find_name_in_mro(type(obj), "__getitem__")
    if hasattr(getitem_callable, "__get__"):
        getitem_callable = getitem_callable.__get__(obj, type(obj))

    return getitem_callable(key)
```

In the following tests, we demonstrate that we implement this behavior correctly.

## Operating on class objects

If we invoke a dunder method on a class, it is looked up on the *meta* class, since any class is an
instance of its metaclass:

```py
class Meta(type):
    def __getitem__(cls, key: int) -> str:
        return str(key)

class DunderOnMetaclass(metaclass=Meta):
    pass

reveal_type(DunderOnMetaclass[0])  # revealed: str
```

If the dunder method is only present on the class itself, it will not be called:

```py
class ClassWithNormalDunder:
    def __getitem__(self, key: int) -> str:
        return str(key)

# error: [not-subscriptable]
ClassWithNormalDunder[0]
```

## Operating on instances

### Attaching dunder methods to instances in methods

When invoking a dunder method on an instance of a class, it is looked up on the class:

```py
class ClassWithNormalDunder:
    def __getitem__(self, key: int) -> str:
        return str(key)

class_with_normal_dunder = ClassWithNormalDunder()

reveal_type(class_with_normal_dunder[0])  # revealed: str
```

Which can be demonstrated by trying to attach a dunder method to an instance, which will not work:

```py
def external_getitem(instance, key: int) -> str:
    return str(key)

class ThisFails:
    def __init__(self):
        self.__getitem__ = external_getitem

this_fails = ThisFails()

# error: [not-subscriptable] "Cannot subscript object of type `ThisFails` with no `__getitem__` method"
reveal_type(this_fails[0])  # revealed: Unknown
```

However, the attached dunder method *can* be called if accessed directly:

```py
reveal_type(this_fails.__getitem__(this_fails, 0))  # revealed: str
```

The instance-level method is also not called when the class-level method is present:

```py
def external_getitem1(instance, key) -> str:
    return "a"

def external_getitem2(key) -> int:
    return 1

def _(flag: bool):
    class ThisFails:
        if flag:
            __getitem__ = external_getitem1

        def __init__(self):
            # error: [invalid-assignment] "Object of type `def external_getitem2(key) -> int` is not assignable to attribute `__getitem__` of type `(instance, key) -> str`"
            self.__getitem__ = external_getitem2

    this_fails = ThisFails()

    # TODO: this would be a friendlier diagnostic if we propagated the error up the stack
    # and transformed it into a `[not-subscriptable]` error with a subdiagnostic explaining
    # that the cause of the error was a possibly missing `__getitem__` method
    #
    # error: [possibly-missing-implicit-call] "Method `__getitem__` of type `ThisFails` may be missing"
    reveal_type(this_fails[0])  # revealed: str
```

### Dunder methods as class-level annotations with no value

Class-level annotations with no value assigned are considered to be accessible on the class:

```py
from typing import Callable

class C:
    __call__: Callable[..., None]

C()()

_: Callable[..., None] = C()
```

The dunder-name heuristic also does not apply to a callable parameterized by a `ParamSpec`, even
after the `ParamSpec` is specialized:

```py
from collections.abc import Callable
from typing import Generic, ParamSpec, Protocol
from typing_extensions import Self

P = ParamSpec("P")

class C(Protocol[P]):
    __call__: Callable[P, int]

def check(value: C[[str]]) -> None:
    reveal_type(value.__call__)  # revealed: (str, /) -> int
    reveal_type(value("value"))  # revealed: int

class Base(Generic[P]):
    __getitem__: Callable[P, Self]

class Child(Base[[int]]):
    pass

def check_self(value: Child) -> None:
    reveal_type(value.__getitem__(0))  # revealed: Child
    reveal_type(value[0])  # revealed: Child

    result: Child = value[0]
```

And of course the same is true if we have only an implicit assignment inside a method:

```py
from typing import Callable

class C:
    def __init__(self):
        self.__call__ = lambda *a, **kw: None

# error: [call-non-callable]
C()()

# error: [invalid-assignment]
_: Callable[..., None] = C()
```

## When the dunder is not a method

A dunder can also be a non-method callable:

```py
class SomeCallable:
    def __call__(self, key: int) -> str:
        return str(key)

class ClassWithNonMethodDunder:
    __getitem__: SomeCallable = SomeCallable()

class_with_callable_dunder = ClassWithNonMethodDunder()

reveal_type(class_with_callable_dunder[0])  # revealed: str
```

## Dunders are looked up using the descriptor protocol

Here, we demonstrate that the descriptor protocol is invoked when looking up a dunder method. Note
that the `instance` argument is on object of type `ClassWithDescriptorDunder`:

```py
from __future__ import annotations

class SomeCallable:
    def __call__(self, key: int) -> str:
        return str(key)

class Descriptor:
    def __get__(self, instance: ClassWithDescriptorDunder, owner: type[ClassWithDescriptorDunder]) -> SomeCallable:
        return SomeCallable()

class ClassWithDescriptorDunder:
    __getitem__: Descriptor = Descriptor()

class_with_descriptor_dunder = ClassWithDescriptorDunder()

reveal_type(class_with_descriptor_dunder[0])  # revealed: str
```

## Dunders cannot be overwritten on instances

If we attempt to overwrite a dunder method on an instance, it does not affect the behavior of
implicit dunder calls:

```py
class C:
    def __getitem__(self, key: int) -> str:
        return str(key)

    def f(self):
        # error: [invalid-assignment]
        self.__getitem__ = None

# This is still fine, and simply calls the `__getitem__` method on the class
reveal_type(C()[0])  # revealed: str
```

## Calling a union of dunder methods

```py
def _(flag: bool):
    class C:
        if flag:
            def __getitem__(self, key: int) -> str:
                return str(key)

        else:
            def __getitem__(self, key: int) -> bytes:
                return bytes()

    c = C()
    reveal_type(c[0])  # revealed: str | bytes

    if flag:
        class D:
            def __getitem__(self, key: int) -> str:
                return str(key)

    else:
        class D:
            def __getitem__(self, key: int) -> bytes:
                return bytes()

    d = D()
    reveal_type(d[0])  # revealed: str | bytes
```

## Calling a union of types without dunder methods

We add instance attributes here to make sure that we don't treat the implicit dunder calls here like
regular method calls.

```py
def external_getitem(instance, key: int) -> str:
    return str(key)

class NotSubscriptable1:
    def __init__(self, value: int):
        self.__getitem__ = external_getitem

class NotSubscriptable2:
    def __init__(self, value: int):
        self.__getitem__ = external_getitem

def _(union: NotSubscriptable1 | NotSubscriptable2):
    # error: [not-subscriptable] "Cannot subscript object of type `NotSubscriptable2` with no `__getitem__` method"
    # error: [not-subscriptable] "Cannot subscript object of type `NotSubscriptable1` with no `__getitem__` method"
    union[0]
```

## Calling a possibly-unbound dunder method

```py
def _(flag: bool):
    class C:
        if flag:
            def __getitem__(self, key: int) -> str:
                return str(key)

    c = C()

    # TODO: this would be a friendlier diagnostic if we propagated the error up the stack
    # and transformed it into a `[not-subscriptable]` error with a subdiagnostic explaining
    # that the cause of the error was a possibly missing `__getitem__` method
    #
    # error: [possibly-missing-implicit-call] "Method `__getitem__` of type `C` may be missing"
    reveal_type(c[0])  # revealed: str
```

## Implicit calls preserve intersection receivers

Implicit calls bind `Self` to the full receiver, just like explicit calls to the same method.

```py
from typing_extensions import Self
from ty_extensions import Intersection

class C:
    def __call__(self) -> Self:
        return self

    def __neg__(self) -> Self:
        return self

    def __pos__(self) -> Self:
        return self

    def __invert__(self) -> Self:
        return self

    def __getitem__(self, key: int | None) -> Self:
        return self

class Other: ...

def narrowed(c: C, key: int | None):
    if isinstance(c, Other):
        reveal_type(c.__call__())  # revealed: C & Other
        reveal_type(c())  # revealed: C & Other
        reveal_type(-c)  # revealed: C & Other
        reveal_type(+c)  # revealed: C & Other
        reveal_type(~c)  # revealed: C & Other
        reveal_type(c[0])  # revealed: C & Other
        reveal_type(c[key])  # revealed: C & Other
    else:
        reveal_type(c())  # revealed: C & ~Other
        reveal_type(-c)  # revealed: C & ~Other

def reversed_order(c: Intersection[Other, C]):
    reveal_type(c())  # revealed: Other & C
    reveal_type(-c)  # revealed: Other & C
```

Each member of a union retains its own intersection receiver, and multiple methods on an
intersection can contribute call signatures without intersecting the bound method objects.

```py
class D:
    def __call__(self) -> Self:
        return self

    def __neg__(self) -> Self:
        return self

def union(c: Intersection[C, Other] | Intersection[D, Other]):
    reveal_type(c())  # revealed: (C & Other) | (D & Other)
    reveal_type(-c)  # revealed: (C & Other) | (D & Other)

def multiple_providers(c: Intersection[C, D]):
    reveal_type(c())  # revealed: C & D
    reveal_type(-c)  # revealed: C & D
```

## Arithmetic preserves intersection receivers

Both normal and reflected operators bind `Self` to the full receiver. Augmented assignment retains
that receiver as well.

```py
from typing_extensions import Self
from ty_extensions import Intersection

class C:
    def __add__(self, other: int) -> Self:
        return self

    def __radd__(self, other: int) -> Self:
        return self

    def __iadd__(self, other: int) -> Self:
        return self

class Other: ...

def _(c: Intersection[C, Other]):
    reveal_type(c + 1)  # revealed: C & Other
    reveal_type(1 + c)  # revealed: C & Other
    c += 1
    reveal_type(c)  # revealed: C & Other
```

## Context managers preserve intersection receivers

```py
from typing import Any
from typing_extensions import Self
from ty_extensions import Intersection

class C:
    def __enter__(self) -> Self:
        return self

    def __exit__(self, *args: Any) -> None: ...
    async def __aenter__(self) -> Self:
        return self

    async def __aexit__(self, *args: Any) -> None: ...

class Other: ...

async def _(c: Intersection[C, Other]):
    with c as entered:
        reveal_type(entered)  # revealed: C & Other
    async with c as entered_async:
        reveal_type(entered_async)  # revealed: C & Other
```

## Await preserves intersection receivers

`Self` can also occur inside the return type of a dunder method.

```py
from typing import Any, Generator
from typing_extensions import Self
from ty_extensions import Intersection

class C:
    def __await__(self) -> Generator[Any, None, Self]:
        yield
        return self

class Other: ...

async def _(c: Intersection[C, Other]):
    reveal_type(await c)  # revealed: C & Other
```

## Iteration over intersection receivers

```py
from typing_extensions import Self
from ty_extensions import Intersection

class C:
    def __iter__(self) -> Self:
        return self

    def __next__(self) -> Self:
        return self

class Other: ...

def _(c: Intersection[C, Other]):
    reveal_type(c.__iter__())  # revealed: C & Other
    reveal_type(c.__next__())  # revealed: C & Other
    # TODO: These should all retain `C & Other`, just like the explicit calls.
    reveal_type(iter(c))  # revealed: C
    reveal_type(next(c))  # revealed: C
    for item in c:
        reveal_type(item)  # revealed: C
```

## Async iteration over intersection receivers

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Self
from ty_extensions import Intersection

class C:
    def __aiter__(self) -> Self:
        return self

    async def __anext__(self) -> Self:
        return self

class Other: ...

async def _(c: Intersection[C, Other]):
    reveal_type(c.__aiter__())  # revealed: C & Other
    reveal_type(await c.__anext__())  # revealed: C & Other
    # TODO: These built-ins should retain `C & Other`, just like the explicit calls.
    reveal_type(aiter(c))  # revealed: C
    reveal_type(await anext(c))  # revealed: C
    async for item in c:
        reveal_type(item)  # revealed: C & Other
```
