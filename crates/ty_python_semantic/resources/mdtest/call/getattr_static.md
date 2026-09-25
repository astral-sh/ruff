# `inspect.getattr_static`

## Basic usage

`inspect.getattr_static` is a function that returns attributes of an object without invoking the
descriptor protocol (for caveats, see the [official documentation]).

Consider the following example:

```py
import inspect

class Descriptor:
    def __get__(self, instance, owner) -> str:
        return "a"

class C:
    normal: int = 1
    descriptor: Descriptor = Descriptor()
```

If we access attributes on an instance of `C` as usual, the descriptor protocol is invoked, and we
get a type of `str` for the `descriptor` attribute:

```py
c = C()

reveal_type(c.normal)  # revealed: int
reveal_type(c.descriptor)  # revealed: str
```

However, if we use `inspect.getattr_static`, we can see the underlying `Descriptor` type:

```py
reveal_type(inspect.getattr_static(c, "normal"))  # revealed: int
reveal_type(inspect.getattr_static(c, "descriptor"))  # revealed: Descriptor
```

For non-existent attributes, a default value can be provided:

```py
reveal_type(inspect.getattr_static(C, "normal", "default-arg"))  # revealed: int
reveal_type(inspect.getattr_static(C, "non_existent", "default-arg"))  # revealed: Literal["default-arg"]
```

When a non-existent attribute is accessed without a default value, the runtime raises an
`AttributeError`. We could emit a diagnostic for this case, but that is currently not supported:

```py
def _():
    # TODO: we could emit a diagnostic here
    reveal_type(inspect.getattr_static(C, "non_existent"))  # revealed: Never
```

We can access attributes on objects of all kinds:

```py
import sys

reveal_type(inspect.getattr_static(sys, "dont_write_bytecode"))  # revealed: bool
# revealed: def getattr_static(obj: object, attr: str, default: Any | None = ...) -> Any
reveal_type(inspect.getattr_static(inspect, "getattr_static"))

reveal_type(inspect.getattr_static(1, "real"))  # revealed: property
```

(Implicit) instance attributes can also be accessed through `inspect.getattr_static`:

```py
class D:
    def __init__(self) -> None:
        self.instance_attr: int = 1

reveal_type(inspect.getattr_static(D(), "instance_attr"))  # revealed: int
```

And attributes on metaclasses can be accessed when probing the class:

```py
class Meta(type):
    attr: int = 1

class E(metaclass=Meta): ...

reveal_type(inspect.getattr_static(E, "attr"))  # revealed: int
```

Metaclass attributes cannot be added when probing an instance of the class:

```py
reveal_type(inspect.getattr_static(E(), "attr", "non_existent"))  # revealed: Literal["non_existent"]
```

## Error cases

We can only infer precise types if the attribute is a literal string. In all other cases, we fall
back to `Any`:

```py
import inspect

class C:
    x: int = 1

def _(attr_name: str):
    reveal_type(inspect.getattr_static(C(), attr_name))  # revealed: Any
    reveal_type(inspect.getattr_static(C(), attr_name, 1))  # revealed: Any
```

But we still detect errors in the number or type of arguments:

```py
# error: [missing-argument] "No arguments provided for required parameters `obj`, `attr` of function `getattr_static`"
inspect.getattr_static()

# error: [missing-argument] "No argument provided for required parameter `attr`"
inspect.getattr_static(C())

# error: [invalid-argument-type] "Argument to function `getattr_static` is incorrect: Expected `str`, found `Literal[1]`"
inspect.getattr_static(C(), 1)

# error: [too-many-positional-arguments] "Too many positional arguments to function `getattr_static`: expected 3, got 4"
inspect.getattr_static(C(), "x", "default-arg", "one too many")
```

## Possibly unbound attributes

```py
import inspect

def _(flag: bool):
    class C:
        if flag:
            x: int = 1

    reveal_type(inspect.getattr_static(C, "x", "default"))  # revealed: int | Literal["default"]
```

## Metaclass fallback for possibly unbound attributes

When a class attribute is absent, static lookup falls back to the metaclass before using the default
argument. A conditionally defined class attribute therefore does not make the result possibly
missing when the metaclass always provides it.

```py
from inspect import getattr_static

class Meta(type):
    attr: int = 1

def check(flag: bool):
    class C(metaclass=Meta):
        if flag:
            attr: str = "class attribute"

    reveal_type(getattr_static(C, "attr", None))  # revealed: str | int
```

## Static lookup on unions

Each alternative uses its own class and metaclass lookup before the results are combined. In
particular, a class attribute on one alternative does not hide the metaclass attribute of another
alternative.

```py
from inspect import getattr_static

class Meta(type):
    attr: int = 1

class A(metaclass=Meta):
    attr: str = "class attribute"

class B(metaclass=Meta): ...

def check(flag: bool):
    cls = A if flag else B
    reveal_type(getattr_static(cls, "attr", None))  # revealed: str | int
```

An instance alternative also participates in lookup:

```py
class C:
    attr: bytes = b"instance attribute"

def check_mixed(flag: bool):
    value = A if flag else C()
    reveal_type(getattr_static(value, "attr", None))  # revealed: str | bytes
```

## Gradual types

```py
import inspect
from typing import Any

def _(a: Any, tuple_of_any: tuple[Any]):
    reveal_type(inspect.getattr_static(a, "x", "default"))  # revealed: Any | Literal["default"]

    # revealed: def index(self, value: Any, start: SupportsIndex = 0, stop: SupportsIndex = ..., /) -> int
    reveal_type(inspect.getattr_static(tuple_of_any, "index", "default"))
```

## Classmethod and staticmethod descriptors

`getattr_static` returns the raw `classmethod` or `staticmethod` descriptor:

```py
from inspect import getattr_static

class C:
    @classmethod
    def some_classmethod(cls) -> int:
        return 1

    @staticmethod
    def some_staticmethod() -> int:
        return 1

some_classmethod = getattr_static(C, "some_classmethod")
some_staticmethod = getattr_static(C, "some_staticmethod")

reveal_type(type(some_classmethod))  # revealed: <class 'classmethod'>
reveal_type(type(some_staticmethod))  # revealed: <class 'staticmethod'>
```

These objects expose the original function through the `__func__` attribute:

```py
reveal_type(some_classmethod.__func__)  # revealed: def some_classmethod(cls) -> int
reveal_type(some_staticmethod.__func__)  # revealed: def some_staticmethod() -> int
```

Attributes like `__kwdefaults__`, which are available on functions, are not directly accessible on
the raw `classmethod` or `staticmethod` descriptors (this is a regression test for
<https://github.com/astral-sh/ty/issues/1452>):

```py
some_classmethod.__kwdefaults__  # error: [unresolved-attribute]
some_staticmethod.__kwdefaults__  # error: [unresolved-attribute]
```

## Builtin `__new__` identity

Unlike Python-defined `__new__` methods, builtin `__new__` methods are not wrapped in a
`staticmethod` descriptor. Static lookup can therefore return the same object as ordinary attribute
access. This comparison guards against custom object allocation and is not redundant.

```toml
[rules]
redundant-condition-strict = "error"
```

```py
import inspect

def check_allocation(cls: object):
    reveal_type(inspect.getattr_static(cls, "__new__"))  # revealed: Any
    reveal_type(inspect.getattr_static(cls, "__new__") is object.__new__)  # revealed: bool
    if inspect.getattr_static(cls, "__new__") is not object.__new__:
        raise TypeError

check_allocation(object)
```

Static and ordinary lookup also agree when the class is known, including when `__new__` is
inherited:

```py
class C: ...

reveal_type(inspect.getattr_static(object, "__new__") is object.__new__)  # revealed: Literal[True]
reveal_type(inspect.getattr_static(C, "__new__") is object.__new__)  # revealed: Literal[True]
reveal_type(inspect.getattr_static(int, "__new__") is int.__new__)  # revealed: Literal[True]
reveal_type(inspect.getattr_static(list, "__new__") is list.__new__)  # revealed: Literal[True]
```

The descriptor can change in a subclass, so a `type` or instance annotation does not establish its
identity:

```py
def check_class(cls: type):
    reveal_type(inspect.getattr_static(cls, "__new__") is object.__new__)  # revealed: bool

def check_subclass(cls: type[C]):
    reveal_type(inspect.getattr_static(cls, "__new__") is object.__new__)  # revealed: bool

def check_instance(instance: C):
    reveal_type(inspect.getattr_static(instance, "__new__") is object.__new__)  # revealed: bool
```

## Builtin `__new__` descriptor behavior

Builtin `__new__` functions have no descriptor wrapper or `__get__` method. Assigning one to a class
attribute does not bind an instance to its first parameter.

```py
from inspect import getattr_static

new = getattr_static(object, "__new__")
reveal_type(type(new))  # revealed: <class 'BuiltinFunctionType'>
new.__func__  # error: [unresolved-attribute]
new.__get__  # error: [unresolved-attribute]

class C:
    create = object.__new__

reveal_type(C.create(C))  # revealed: C
reveal_type(C().create(C))  # revealed: C
```

Generalizing a builtin function to a callable preserves its runtime class and binding behavior:

```py
constructors = [object.__new__]
reveal_type(type(constructors[0]))  # revealed: <class 'BuiltinFunctionType'>

class D:
    create = constructors[0]

class Child(D): ...

reveal_type(D().create(D))  # revealed: D
reveal_type(D().create(Child))  # revealed: Child
```

Explicitly wrapping a builtin function in `staticmethod` creates a separate descriptor that exposes
the original builtin function:

```py
wrapped = staticmethod(object.__new__)
reveal_type(type(wrapped))  # revealed: <class 'staticmethod'>
reveal_type(type(wrapped.__func__))  # revealed: <class 'BuiltinFunctionType'>
reveal_type(wrapped.__func__ is object.__new__)  # revealed: Literal[True]
```

## Final-class `__new__` descriptors

An instance of a final class cannot inherit a different constructor from a subclass, so static
lookup preserves its `__new__` signature and checks the arguments when it is called.

```py
from inspect import getattr_static
from typing import final

@final
class C:
    def __new__(cls, value: int):
        return object.__new__(cls)

def check(instance: C):
    new = getattr_static(instance, "__new__")
    reveal_type(type(new))  # revealed: <class 'staticmethod'>
    new(C, 1)
    new(C, "wrong")  # error: [invalid-argument-type]
```

## Static `__new__` lookup on unions

Unions of known classes retain the signature of their shared constructor:

```py
from inspect import getattr_static

class Base:
    def __new__(cls, value: int):
        return object.__new__(cls)

class C(Base): ...
class D(Base): ...

def check(flag: bool):
    cls = C if flag else D
    new = getattr_static(cls, "__new__")
    new(cls, 1)
    new(cls, "wrong")  # error: [invalid-argument-type]
```

## Python-defined `__new__` descriptors

Python-defined `__new__` methods are implicitly wrapped in `staticmethod`, so static and ordinary
lookup return distinct objects.

```py
from inspect import getattr_static

class C:
    def __new__(cls):
        return object.__new__(cls)

new = getattr_static(C, "__new__")
reveal_type(type(new))  # revealed: <class 'staticmethod'>
reveal_type(new is C.__new__)  # revealed: Literal[False]
reveal_type(new.__func__ is C.__new__)  # revealed: Literal[True]
```

[official documentation]: https://docs.python.org/3/library/inspect.html#inspect.getattr_static
