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

Since every class has `object` in it's MRO, the default implementations are `object.__new__` and
`object.__init__`. They have some special behavior, namely:

- If neither `__new__` nor `__init__` are defined anywhere in the MRO of class (except for
    `object`), no arguments are accepted and `TypeError` is raised if any are passed.
- If `__new__` is defined but `__init__` is not, `object.__init__` will allow arbitrary arguments!

As of today there are a number of behaviors that we do not support:

- `__new__` is assumed to return an instance of the class on which it is called
- User defined `__call__` on metaclass is ignored

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

# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 1, got 2"
reveal_type(Foo(1))  # revealed: Foo
```

## `__new__` present on the class itself

```py
class Foo:
    def __new__(cls, x: int) -> "Foo":
        return object.__new__(cls)

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to function `__new__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## `__new__` present on a superclass

If the `__new__` method is defined on a superclass, we can still infer the signature of the
constructor from it.

```py
from typing_extensions import Self

class Base:
    def __new__(cls, x: int) -> Self:
        return cls()

class Foo(Base): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to function `__new__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
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
    # error: [invalid-argument-type] "Argument to function `__new__` is incorrect: Expected `int`, found `Literal["1"]`"
    # error: [invalid-argument-type] "Argument to function `__new__` is incorrect: Expected `int`, found `Literal["1"]`"
    reveal_type(Foo("1"))  # revealed: Foo
    # error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
    # error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
    reveal_type(Foo())  # revealed: Foo
    # error: [too-many-positional-arguments] "Too many positional arguments to function `__new__`: expected 2, got 3"
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
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
reveal_type(Foo())  # revealed: Foo
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
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
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

    # error: [possibly-unbound-implicit-call]
    reveal_type(Foo())  # revealed: Foo

    # error: [possibly-unbound-implicit-call]
    # error: [too-many-positional-arguments]
    reveal_type(Foo(1))  # revealed: Foo
```

#### Possibly unbound `__call__` on `__new__` callable

```py
def _(flag: bool) -> None:
    class Callable:
        if flag:
            def __call__(self, cls, x: int) -> "Foo":
                return object.__new__(cls)

    class Foo:
        __new__ = Callable()

    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly unbound `__call__` method)"
    reveal_type(Foo(1))  # revealed: Foo
    # TODO should be - error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
    # but we currently infer the signature of `__call__` as unknown, so it accepts any arguments
    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly unbound `__call__` method)"
    reveal_type(Foo())  # revealed: Foo
```

## `__init__` present on the class itself

If the class has an `__init__` method, we can infer the signature of the constructor from it.

```py
class Foo:
    def __init__(self, x: int): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## `__init__` present on a superclass

If the `__init__` method is defined on a superclass, we can still infer the signature of the
constructor from it.

```py
class Base:
    def __init__(self, x: int): ...

class Foo(Base): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 2, got 3"
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
    # error: [invalid-argument-type] "Argument to bound method `__init__` is incorrect: Expected `int`, found `Literal["1"]`"
    # error: [invalid-argument-type] "Argument to bound method `__init__` is incorrect: Expected `int`, found `Literal["1"]`"
    reveal_type(Foo("1"))  # revealed: Foo
    # error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
    # error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
    reveal_type(Foo())  # revealed: Foo
    # error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 2, got 3"
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
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
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
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
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

    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly unbound `__call__` method)"
    reveal_type(Foo(1))  # revealed: Foo
    # TODO should be - error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
    # but we currently infer the signature of `__call__` as unknown, so it accepts any arguments
    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly unbound `__call__` method)"
    reveal_type(Foo())  # revealed: Foo
```

## `__new__` and `__init__` both present

### Identical signatures

A common case is to have `__new__` and `__init__` with identical signatures (except for the first
argument). We report errors for both `__new__` and `__init__` if the arguments are incorrect.

At runtime `__new__` is called first and will fail without executing `__init__` if the arguments are
incorrect. However, we decided that it is better to report errors for both methods, since after
fixing the `__new__` method, the user may forget to fix the `__init__` method.

```py
class Foo:
    def __new__(cls, x: int) -> "Foo":
        return object.__new__(cls)

    def __init__(self, x: int): ...

# error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
reveal_type(Foo())  # revealed: Foo

reveal_type(Foo(1))  # revealed: Foo
```

### Compatible signatures

But they can also be compatible, but not identical. We should correctly report errors only for the
mthod that would fail.

```py
class Foo:
    def __new__(cls, *args, **kwargs):
        return object.__new__(cls)

    def __init__(self, x: int) -> None:
        self.x = x

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
reveal_type(Foo())  # revealed: Foo
reveal_type(Foo(1))  # revealed: Foo

# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 2, got 3"
reveal_type(Foo(1, 2))  # revealed: Foo
```

### Incompatible signatures

```py
import abc

class Foo:
    def __new__(cls) -> "Foo":
        return object.__new__(cls)

    def __init__(self, x):
        self.x = 42

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
reveal_type(Foo())  # revealed: Foo

# error: [too-many-positional-arguments] "Too many positional arguments to function `__new__`: expected 1, got 2"
reveal_type(Foo(42))  # revealed: Foo

class Foo2:
    def __new__(cls, x) -> "Foo2":
        return object.__new__(cls)

    def __init__(self):
        pass

# error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
reveal_type(Foo2())  # revealed: Foo2

# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 1, got 2"
reveal_type(Foo2(42))  # revealed: Foo2

class Foo3(metaclass=abc.ABCMeta):
    def __new__(cls) -> "Foo3":
        return object.__new__(cls)

    def __init__(self, x):
        self.x = 42

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
reveal_type(Foo3())  # revealed: Foo3

# error: [too-many-positional-arguments] "Too many positional arguments to function `__new__`: expected 1, got 2"
reveal_type(Foo3(42))  # revealed: Foo3

class Foo4(metaclass=abc.ABCMeta):
    def __new__(cls, x) -> "Foo4":
        return object.__new__(cls)

    def __init__(self):
        pass

# error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
reveal_type(Foo4())  # revealed: Foo4

# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 1, got 2"
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
