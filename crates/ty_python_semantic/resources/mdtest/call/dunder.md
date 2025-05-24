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

# error: [non-subscriptable]
ClassWithNormalDunder[0]
```

## Operating on instances

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

# error: [non-subscriptable] "Cannot subscript object of type `ThisFails` with no `__getitem__` method"
reveal_type(this_fails[0])  # revealed: Unknown
```

However, the attached dunder method *can* be called if accessed directly:

```py
reveal_type(this_fails.__getitem__(this_fails, 0))  # revealed: Unknown | str
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
            self.__getitem__ = external_getitem2

    this_fails = ThisFails()

    # error: [possibly-unbound-implicit-call]
    reveal_type(this_fails[0])  # revealed: Unknown | str
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

## Dunders can not be overwritten on instances

If we attempt to overwrite a dunder method on an instance, it does not affect the behavior of
implicit dunder calls:

```py
class C:
    def __getitem__(self, key: int) -> str:
        return str(key)

    def f(self):
        # TODO: This should emit an `invalid-assignment` diagnostic once we understand the type of `self`
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
    # error: [non-subscriptable]
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
    # error: [possibly-unbound-implicit-call]
    reveal_type(c[0])  # revealed: str
```

## Dunder methods cannot be looked up on instances

Class-level annotations with no value assigned are considered instance-only, and aren't available as
dunder methods:

```py
from typing import Callable

class C:
    __call__: Callable[..., None]

# error: [call-non-callable]
C()()

# error: [invalid-assignment]
_: Callable[..., None] = C()
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
