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

An explicitly annotated callable parameterized by a `ParamSpec` remains regular, even after the
`ParamSpec` is specialized:

```py
from collections.abc import Callable
from typing import Generic, ParamSpec, Protocol, TypeVar
from typing_extensions import Self

P = ParamSpec("P")

class C(Protocol[P]):
    __call__: Callable[P, int]

def check(value: C[[str]]) -> None:
    reveal_type(value.__call__)  # revealed: (str, /) -> int
    reveal_type(value("value"))  # revealed: int

Call = TypeVar("Call", bound=Callable[..., object])

class CallableWrapper(Protocol[Call]):
    __call__: Call

def decorate(function: Call) -> CallableWrapper[Call]:
    raise NotImplementedError

def view(request: object, kind: str) -> object:
    return request, kind

decorated = decorate(view)
reveal_type(decorated.__call__)  # revealed: (request: object, kind: str) -> object
reveal_type(decorated(object(), kind="task"))  # revealed: object

class Base(Generic[P]):
    __getitem__: Callable[P, Self]

class Child(Base[[int]]):
    pass

def check_self(value: Child) -> None:
    reveal_type(value.__getitem__(0))  # revealed: Child
    reveal_type(value[0])  # revealed: Child

    result: Child = value[0]
```

### Implicit descriptor binding for annotated callables

If no overload of an annotated callable matches the arguments of an implicit dunder call and an
overload requires an additional argument, its first parameter binds the receiver. This includes
annotations that refine methods inherited from a base class, while direct attribute access still
exposes the unbound callable:

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

from collections.abc import Callable, Iterator
from typing import Any

class Operand:
    def __add__(self, value: Any, /) -> Any: ...
    def __radd__(self, value: Any, /) -> Any: ...
    def __iadd__(self, value: Any, /) -> Any: ...
    def __neg__(self) -> Any: ...
    def __eq__(self, value: Any, /) -> Any: ...
    def __contains__(self, value: Any, /) -> Any: ...
    def __getitem__(self, value: Any, /) -> Any: ...
    def __setitem__(self, key: Any, value: Any, /) -> Any: ...
    def __iter__(self) -> Any: ...
    def __enter__(self) -> Any: ...
    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any, /) -> Any: ...

class Index(Operand):
    __add__: Callable[["Index", Any], "Index"]
    __radd__: Callable[["Index", Any], "Index"]
    __iadd__: Callable[["Index", Any], "Index"]
    __neg__: Callable[["Index"], "Index"]
    __eq__: Callable[["Index", Any], list[bool]]
    __contains__: Callable[["Index", Any], bool]
    __getitem__: Callable[["Index", Any], str]
    __setitem__: Callable[["Index", Any, str], None]
    __iter__: Callable[["Index"], Iterator[int]]
    __enter__: Callable[["Index"], "Index"]
    __exit__: Callable[["Index", Any, Any, Any], bool]

index = Index()
reveal_type(index.__add__)  # revealed: (Index, Any, /) -> Index
reveal_type(index + 1)  # revealed: Index
reveal_type(1 + index)  # revealed: Index
reveal_type(-index)  # revealed: Index
reveal_type(index == 1)  # revealed: list[bool]
reveal_type(1 in index)  # revealed: bool
reveal_type(index[1])  # revealed: str
index[1] = "value"

for value in index:
    reveal_type(value)  # revealed: int

with index as entered:
    reveal_type(entered)  # revealed: Index

index += 1
reveal_type(index)  # revealed: Index

def add(value: Direct, other: int, /) -> Direct:
    return value

class Direct:
    __add__: Callable[[Direct, int], Direct] = add

reveal_type(Direct().__add__)  # revealed: (Direct, int, /) -> Direct
reveal_type(Direct() + 1)  # revealed: Direct

type AliasedAdd = Callable[[Aliased, int], Aliased]

class Aliased:
    __add__: AliasedAdd

reveal_type(Aliased() + 1)  # revealed: Aliased

class AddDescriptor:
    def __get__(self, instance: WithDescriptor, owner: type[WithDescriptor], /) -> Callable[[int], str]:
        raise NotImplementedError

class WithDescriptor:
    __add__: AddDescriptor = AddDescriptor()

reveal_type(WithDescriptor() + 1)  # revealed: str

class KeywordMapping:
    __getitem__: Callable[[KeywordMapping, str], int]

    def keys(self) -> list[str]:
        raise NotImplementedError

def accepts_keywords(**kwargs: int) -> None: ...

accepts_keywords(**KeywordMapping())
```

Callables whose signatures already accept the implicit arguments remain regular, which supports
class members implemented by callable objects:

```py
class BinaryCallable:
    def __call__(self, value: int, /) -> str:
        return str(value)

class UnaryCallable:
    def __call__(self) -> str:
        return "called"

type RegularBinary = Callable[[int], str]

class WithCallableObjects:
    __add__: RegularBinary = BinaryCallable()
    __contains__: Callable[[int], str] = BinaryCallable()
    __getitem__: Callable[[int], str] = BinaryCallable()
    __neg__: Callable[[], str] = UnaryCallable()

callable_objects = WithCallableObjects()
reveal_type(callable_objects + 1)  # revealed: str
reveal_type(1 in callable_objects)  # revealed: bool
reveal_type(callable_objects[1])  # revealed: str
reveal_type(-callable_objects)  # revealed: str

def check_mixed_union(value: Index | WithCallableObjects) -> None:
    reveal_type(value + 1)  # revealed: Index | str
```

### Dunder methods attached to instances

Dunder methods assigned to an instance inside a method cannot be called implicitly:

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

## Intersection receivers

```toml
[environment]
python-version = "3.14"
```

### Calls and unary operators

Calls through `__call__` and unary operators bind `Self` to the full receiver, just like explicit
calls to the same method.

```py
from typing import Self

class C:
    def __call__(self) -> Self:
        return self

    def __neg__(self) -> Self:
        return self

    def __pos__(self) -> Self:
        return self

    def __invert__(self) -> Self:
        return self

class Other: ...

def narrowed(c: C):
    if isinstance(c, Other):
        reveal_type(c.__call__())  # revealed: C & Other
        reveal_type(c())  # revealed: C & Other

        reveal_type(c.__neg__())  # revealed: C & Other
        reveal_type(-c)  # revealed: C & Other

        reveal_type(c.__pos__())  # revealed: C & Other
        reveal_type(+c)  # revealed: C & Other

        reveal_type(c.__invert__())  # revealed: C & Other
        reveal_type(~c)  # revealed: C & Other
```

Each member of a union retains its own intersection receiver, and multiple methods on an
intersection can contribute call signatures without intersecting the bound method objects.

```py
class D:
    def __call__(self) -> Self:
        return self

    def __neg__(self) -> Self:
        return self

def union(c: C & Other | D & Other):
    reveal_type(c.__call__())  # revealed: (C & Other) | (D & Other)
    reveal_type(c())  # revealed: (C & Other) | (D & Other)

    reveal_type(c.__neg__())  # revealed: (C & Other) | (D & Other)
    reveal_type(-c)  # revealed: (C & Other) | (D & Other)
```

### Arithmetic

Both normal and reflected operators bind `Self` to the full receiver. Augmented assignment retains
that receiver as well.

```py
from typing import Self

class C:
    def __add__(self, other: int) -> Self:
        return self

    def __radd__(self, other: int) -> Self:
        return self

    def __iadd__(self, other: int) -> Self:
        return self

class Other: ...

def _(c: C & Other):
    reveal_type(c.__add__(1))  # revealed: C & Other
    reveal_type(c + 1)  # revealed: C & Other

    reveal_type(c.__radd__(1))  # revealed: C & Other
    reveal_type(1 + c)  # revealed: C & Other

    reveal_type(c.__iadd__(1))  # revealed: C & Other
    c += 1
    reveal_type(c)  # revealed: C & Other
```

### Subscripting

```py
from typing import Self

class C:
    def __getitem__(self, key: int) -> Self:
        return self

class Other: ...

def _(c: C & Other):
    reveal_type(c.__getitem__(0))  # revealed: C & Other
    # TODO: This should retain `C & Other`, just like the explicit call.
    reveal_type(c[0])  # revealed: C
```

### Context managers

```py
from typing import Any, Self

class C:
    def __enter__(self) -> Self:
        return self

    def __exit__(self, *args: Any) -> None: ...
    async def __aenter__(self) -> Self:
        return self

    async def __aexit__(self, *args: Any) -> None: ...

class Other: ...

async def _(c: C & Other):
    reveal_type(c.__enter__())  # revealed: C & Other
    with c as entered:
        reveal_type(entered)  # revealed: C & Other

    reveal_type(await c.__aenter__())  # revealed: C & Other
    async with c as entered_async:
        reveal_type(entered_async)  # revealed: C & Other
```

### Await

`Self` can also occur inside the return type of a dunder method.

```py
from typing import Any, Generator, Self

class C:
    def __await__(self) -> Generator[Any, None, Self]:
        yield
        return self

class Other: ...

async def _(c: C & Other):
    reveal_type(c.__await__())  # revealed: Generator[Any, None, C & Other]
    reveal_type(await c)  # revealed: C & Other
```

### Iteration

#### Synchronous

```py
from typing import Self

class C:
    def __iter__(self) -> Self:
        return self

    def __next__(self) -> Self:
        return self

class Other: ...

def _(c: C & Other):
    reveal_type(c.__iter__())  # revealed: C & Other
    # TODO: This should retain `C & Other`, just like the explicit call.
    reveal_type(iter(c))  # revealed: C

    reveal_type(c.__next__())  # revealed: C & Other
    # TODO: This should retain `C & Other`, just like the explicit call.
    reveal_type(next(c))  # revealed: C

    reveal_type(c.__iter__().__next__())  # revealed: C & Other
    for item in c:
        # TODO: This should retain `C & Other`, just like the explicit calls.
        reveal_type(item)  # revealed: C
```

#### Asynchronous

```py
from typing import Self

class C:
    def __aiter__(self) -> Self:
        return self

    async def __anext__(self) -> Self:
        return self

class Other: ...

async def _(c: C & Other):
    reveal_type(c.__aiter__())  # revealed: C & Other
    # TODO: This should retain `C & Other`, just like the explicit call.
    reveal_type(aiter(c))  # revealed: C

    reveal_type(await c.__anext__())  # revealed: C & Other
    # TODO: This should retain `C & Other`, just like the explicit call.
    reveal_type(await anext(c))  # revealed: C

    reveal_type(await c.__aiter__().__anext__())  # revealed: C & Other
    async for item in c:
        reveal_type(item)  # revealed: C & Other
```
