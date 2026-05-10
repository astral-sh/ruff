# Decorators

Decorators are a way to modify function and class behavior. A decorator is a callable that takes the
function or class as an argument and returns a modified version of it.

## Basic example

A decorated function definition is conceptually similar to `def f(x): ...` followed by
`f = decorator(f)`. This means that the type of a decorated function is the same as the return type
of the decorator (which does not necessarily need to be a callable type):

```py
def custom_decorator(f) -> int:
    return 1

@custom_decorator
def f(x): ...

reveal_type(f)  # revealed: int
```

## Type-annotated decorator

More commonly, a decorator returns a modified callable type:

```py
from typing import Callable

def ensure_positive(wrapped: Callable[[int], bool]) -> Callable[[int], bool]:
    return lambda x: wrapped(x) and x > 0

@ensure_positive
def even(x: int) -> bool:
    return x % 2 == 0

reveal_type(even)  # revealed: (int, /) -> bool
reveal_type(even(4))  # revealed: bool
```

## Decorators which take arguments

Decorators can be arbitrary expressions. This is often useful when the decorator itself takes
arguments:

```py
from typing import Callable

def ensure_larger_than(lower_bound: int) -> Callable[[Callable[[int], bool]], Callable[[int], bool]]:
    def decorator(wrapped: Callable[[int], bool]) -> Callable[[int], bool]:
        return lambda x: wrapped(x) and x >= lower_bound
    return decorator

@ensure_larger_than(10)
def even(x: int) -> bool:
    return x % 2 == 0

reveal_type(even)  # revealed: (int, /) -> bool
reveal_type(even(14))  # revealed: bool
```

Decorator expressions can also introduce bindings that remain visible after the decorated
definition:

```py
def decorator_factory(flag: bool):
    def decorator(func):
        return func
    return decorator

@decorator_factory(seen := True)
def f():
    pass

reveal_type(seen)  # revealed: Literal[True]
```

## Multiple decorators

Multiple decorators can be applied to a single function. They are applied in "bottom-up" order,
meaning that the decorator closest to the function definition is applied first:

```py
def maps_to_str(f) -> str:
    return "a"

def maps_to_int(f) -> int:
    return 1

def maps_to_bytes(f) -> bytes:
    return b"a"

@maps_to_str
@maps_to_int
@maps_to_bytes
def f(x): ...

reveal_type(f)  # revealed: str
```

## Decorating with a class

When a function is decorated with a class-based decorator, the decorated function turns into an
instance of the class (see also: [properties](properties.md)). Attributes of the class can be
accessed on the decorated function.

```py
class accept_strings:
    custom_attribute: str = "a"

    def __init__(self, f):
        self.f = f

    def __call__(self, x: str | int) -> bool:
        return self.f(int(x))

@accept_strings
def even(x: int) -> bool:
    return x > 0

reveal_type(even)  # revealed: accept_strings
reveal_type(even.custom_attribute)  # revealed: str
reveal_type(even("1"))  # revealed: bool
reveal_type(even(1))  # revealed: bool

# error: [invalid-argument-type]
even(None)
```

## Common decorator patterns

### `functools.wraps`

This test mainly makes sure that we do not emit any diagnostics in a case where the decorator is
implemented using `functools.wraps`.

```py
from typing import Callable
from functools import wraps

def custom_decorator(f) -> Callable[[int], str]:
    @wraps(f)
    def wrapper(*args, **kwargs):
        print("Calling decorated function")
        return f(*args, **kwargs)
    return wrapper

@custom_decorator
def f(x: int) -> str:
    return str(x)

reveal_type(f)  # revealed: (int, /) -> str
```

### `functools.cache`

```py
from functools import cache

@cache
def f(x: int) -> int:
    return x**2

# revealed: _lru_cache_wrapper[int]
reveal_type(f)
# revealed: int
reveal_type(f(1))
```

### `functools.cached_property`

```py
from functools import cached_property

class Foo:
    @cached_property
    def foo(self) -> str:
        return "a"

reveal_type(Foo().foo)  # revealed: str
```

## Lambdas as decorators

```py
@lambda f: f
def g(x: int) -> str:
    return "a"

# TODO: This should be `Literal[g]` or `(int, /) -> str`
reveal_type(g)  # revealed: Unknown
```

## Error cases

### Unknown decorator

```py
# error: [unresolved-reference] "Name `unknown_decorator` used when not defined"
@unknown_decorator
def f(x): ...

reveal_type(f)  # revealed: Unknown
```

### Error in the decorator expression

```py
# error: [unsupported-operator]
@(1 + "a")
def f(x): ...

reveal_type(f)  # revealed: Unknown
```

### Non-callable decorator

```py
non_callable = 1

# error: [call-non-callable] "Object of type `Literal[1]` is not callable"
@non_callable
def f(x): ...

reveal_type(f)  # revealed: Unknown
```

### Wrong signature

#### Wrong argument type

Here, we emit a diagnostic since `wrong_signature` takes an `int` instead of a callable type as the
first argument:

```py
def wrong_signature(f: int) -> str:
    return "a"

# error: [invalid-argument-type] "Argument to function `wrong_signature` is incorrect: Expected `int`, found `def f(x) -> Unknown`"
@wrong_signature
def f(x): ...

reveal_type(f)  # revealed: str
```

#### Wrong number of arguments

Decorators need to be callable with a single argument. If they are not, we emit a diagnostic:

```py
def takes_two_arguments(f, g) -> str:
    return "a"

# error: [missing-argument] "No argument provided for required parameter `g` of function `takes_two_arguments`"
@takes_two_arguments
def f(x): ...

reveal_type(f)  # revealed: str

def takes_no_argument() -> str:
    return "a"

# error: [too-many-positional-arguments] "Too many positional arguments to function `takes_no_argument`: expected 0, got 1"
@takes_no_argument
def g(x): ...
```

### Class, with wrong signature, used as a decorator

When a class is used as a decorator, its constructor (`__init__` or `__new__`) must accept the
decorated function as an argument. If the class's constructor doesn't accept the right arguments, we
emit an error:

```py
class NoInit: ...

# error: [too-many-positional-arguments] "Too many positional arguments to `object.__init__`: expected 1, got 2"
@NoInit
def foo(): ...

reveal_type(foo)  # revealed: NoInit

# error: [invalid-argument-type]
@int
def bar(): ...

reveal_type(bar)  # revealed: int
```

### Class, with correct signature, used as a decorator

When a class's constructor accepts the decorated function/class, no error is emitted:

```py
from typing import Callable

class Wrapper:
    def __init__(self, func: Callable[..., object]) -> None:
        self.func = func

@Wrapper
def my_func() -> int:
    return 42

reveal_type(my_func)  # revealed: Wrapper

class AcceptsType:
    def __init__(self, cls: type) -> None:
        self.cls = cls

# Decorator call is validated, but the type transformation isn't applied yet.
# TODO: Class decorator return types should transform the class binding type.
@AcceptsType
class MyClass: ...

reveal_type(MyClass)  # revealed: AcceptsType
```

### Generic class, used as a decorator

Generic class decorators are validated through constructor calls:

```py
from typing import Generic, TypeVar, Callable

T = TypeVar("T")

class Box(Generic[T]):
    def __init__(self, value: T) -> None:
        self.value = value

# error: [invalid-argument-type]
@Box[int]
def returns_str() -> str:
    return "hello"
```

### `type[SomeClass]` used as a decorator

Using `type[SomeClass]` as a decorator validates against the class's constructor:

```py
class Base: ...

def apply_decorator(cls: type[Base]) -> None:
    # error: [too-many-positional-arguments] "Too many positional arguments to `object.__init__`: expected 1, got 2"
    @cls
    def inner() -> None: ...
```

## Class decorators

Class decorator calls are validated, emitting diagnostics for invalid arguments:

```py
def takes_int(x: int) -> int:
    return x

# error: [invalid-argument-type]
@takes_int
class Foo: ...
```

Using `None` as a decorator is an error:

```py
# error: [call-non-callable]
@None
class Bar: ...
```

A decorator can enforce type constraints on the class being decorated:

```py
def decorator(cls: type[int]) -> type[int]:
    return cls

# error: [invalid-argument-type]
@decorator
class Baz: ...

reveal_type(Baz)  # revealed: type[int]
```

Class decorators can also replace the class object with an instance:

```py
from dataclasses import dataclass
from typing import Generic, Protocol, TypeVar, overload
from typing_extensions import Self

T = TypeVar("T")

class Backend(Protocol):
    def get(self, key: str) -> bytes | None: ...

class WrapBackend:
    def __init__(self, cls: type[object]) -> None:
        self.cls = cls

    def get(self, key: str) -> bytes | None:
        return None

@WrapBackend
class CacheClient:
    def clone(self) -> Self:
        reveal_type(self)  # revealed: Self@clone
        return self

    @classmethod
    def make(cls) -> Self:
        reveal_type(cls)  # revealed: type[Self@make]
        return cls()

reveal_type(CacheClient)  # revealed: WrapBackend
reveal_type(CacheClient.get("x"))  # revealed: bytes | None

@WrapBackend
@dataclass
class DataclassThenWrapped:
    value: int

reveal_type(DataclassThenWrapped)  # revealed: WrapBackend

# error: [no-matching-overload]
@dataclass
@WrapBackend
class WrappedThenDataclass:
    value: int

reveal_type(WrappedThenDataclass)  # revealed: Unknown

@WrapBackend
class InvalidWrappedBase(1): ...  # error: [invalid-base]

@WrapBackend
class GenericCacheClient(Generic[T]):
    value: T

    def get_value(self) -> T:
        return self.value

reveal_type(GenericCacheClient)  # revealed: WrapBackend

@WrapBackend
class OverloadedCacheClient:
    @overload
    # error: [invalid-overload] "Overloads for function `get` must be followed by a non-`@overload`-decorated implementation function"
    def get(self, key: str) -> bytes: ...
    @overload
    def get(self, key: bytes) -> bytes: ...
```

Unannotated class decorators are assumed to preserve the class binding. We do not infer returned
classes from decorator bodies:

```py
def personify(cls):
    class Wrapped(cls):
        full_name: str

        def set_full_name(self, full_name: str) -> None:
            self.full_name = full_name

    return Wrapped

@personify
class Animal: ...

reveal_type(Animal)  # revealed: <class 'Animal'>
reveal_type(Animal())  # revealed: Animal

Animal().set_full_name("John")  # error: [unresolved-attribute]
```

This also applies to unannotated callables that are not function definitions:

```py
lambda_decorator = lambda cls: cls

@lambda_decorator
class LambdaDecorated: ...

reveal_type(LambdaDecorated)  # revealed: <class 'LambdaDecorated'>

class DecoratorFactory:
    def decorator(self, cls):
        return cls

decorator_factory = DecoratorFactory()

@decorator_factory.decorator
class BoundMethodDecorated: ...

reveal_type(BoundMethodDecorated)  # revealed: <class 'BoundMethodDecorated'>

class CallableDecorator:
    def __call__(self, cls):
        return cls

callable_decorator = CallableDecorator()

@callable_decorator
class CallableInstanceDecorated: ...

reveal_type(CallableInstanceDecorated)  # revealed: <class 'CallableInstanceDecorated'>

class ExplicitReturnDecorator(Generic[T]):
    def __call__(self, cls) -> T:
        raise NotImplementedError

explicit_return_decorator = ExplicitReturnDecorator()

@explicit_return_decorator
class ExplicitReturnCallableInstanceDecorated: ...

reveal_type(ExplicitReturnCallableInstanceDecorated)  # revealed: Unknown
```

An unknown class decorator still makes the class binding unknown:

```py
# error: [unresolved-reference] "Name `unknown_class_decorator` used when not defined"
@unknown_class_decorator
class UnknownDecorated: ...

reveal_type(UnknownDecorated)  # revealed: Unknown
```

An unannotated class decorator preserves the result of earlier decorators:

```py
def unannotated_identity(cls):
    return cls

@unannotated_identity
@WrapBackend
class WrappedThenUnannotated: ...

reveal_type(WrappedThenUnannotated)  # revealed: WrapBackend
```

Metadata decorators still apply above an unannotated class-preserving decorator:

```py
from typing_extensions import deprecated

def unannotated_identity(cls):
    return cls

@deprecated("use OtherClass")
@unannotated_identity
class DeprecatedThenUnannotated: ...

DeprecatedThenUnannotated()  # error: [deprecated] "use OtherClass"
```

If a class decorator returns the original class object, we preserve the class binding so it can
still be used in annotations and as a base class:

```py
from typing import TypeVar

T = TypeVar("T", bound=object)

def identity_class_decorator(cls: type[T]) -> type[T]:
    return cls

@identity_class_decorator
class PreservedClass: ...

reveal_type(PreservedClass)  # revealed: <class 'PreservedClass'>

class DerivedPreservedClass(PreservedClass):
    value: PreservedClass
```

Class decorators can return intersections that expose attributes added to the decorated class
object:

```py
from ty_extensions import Intersection
from typing import Protocol, TypeVar

class Resource:
    def fetch(self) -> str:
        return "data"

class ResourceEnabled(Protocol):
    resource: Resource

SchemaT = TypeVar("SchemaT")

def register(cls: type[SchemaT]) -> Intersection[type[SchemaT], ResourceEnabled]:
    return cls

@register
class UserSchema:
    id: int

reveal_type(UserSchema.resource.fetch())  # revealed: str
```
