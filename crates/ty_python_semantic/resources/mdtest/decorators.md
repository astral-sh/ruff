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
from typing import TypeVar

T = TypeVar("T")

def decorator_factory(flag: bool) -> Callable[[T], T]:
    def decorator(func: T) -> T:
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

### `functools.cached_property` on a generic class

A cached property must preserve the type variable bound by its enclosing generic class, including
when the return type is a union:

```py
from functools import cached_property
from typing import Generic, TypeVar

T = TypeVar("T")

class Box(Generic[T]):
    @cached_property
    def value(self) -> T:
        raise NotImplementedError

    @cached_property
    def values(self) -> list[T] | None:
        raise NotImplementedError

reveal_type(Box[int]().value)  # revealed: int
reveal_type(Box[int]().values)  # revealed: list[int] | None
```

## Lambdas as decorators

```py
# TODO: infer the `lambda` as a generic function and avoid the false-positive diagnostic here:
# error: [dynamic-function-decorator-return]
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
# error: [dynamic-function-decorator-return]
@unknown_decorator
def f(x): ...

reveal_type(f)  # revealed: Unknown
```

### Error in the decorator expression

```py
# error: [unsupported-operator]
# error: [dynamic-function-decorator-return]
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
from typing import Callable, Generic, Protocol, TypeVar, overload
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

def int_decorator_factory() -> Callable[[type[object]], int]:
    def decorator(cls: type[object]) -> int:
        return 1
    return decorator

# error: [no-matching-overload]
@dataclass
@int_decorator_factory()
class IntThenDataclass:
    value: int

reveal_type(IntThenDataclass)  # revealed: Unknown

@WrapBackend
class InvalidWrappedBase(1): ...  # error: [invalid-base]

reveal_type(InvalidWrappedBase)  # revealed: WrapBackend

@WrapBackend
class GenericCacheClient(Generic[T]):
    value: T

    def get_value(self) -> T:
        return self.value

reveal_type(GenericCacheClient)  # revealed: WrapBackend

@WrapBackend
class OverloadedCacheClient:
    @overload
    def get(self, key: str) -> bytes: ...
    @overload
    def get(self, key: bytes) -> bytes: ...
    def get(self, key: str | bytes) -> bytes:
        return b""
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

specialized_explicit_return_decorator = ExplicitReturnDecorator[int]()

@specialized_explicit_return_decorator
class SpecializedExplicitReturnCallableInstanceDecorated: ...

reveal_type(SpecializedExplicitReturnCallableInstanceDecorated)  # revealed: int

def function_decorator(func: Callable[..., T]) -> Callable[..., T]:
    return func

@function_decorator
def explicit_return_callable_decorator(cls) -> T:
    raise NotImplementedError

@explicit_return_callable_decorator
class ExplicitReturnCallableDecorated: ...

reveal_type(ExplicitReturnCallableDecorated)  # revealed: Unknown

def regular_callable_replacement_factory() -> Callable[[type[object]], T]:
    raise NotImplementedError

@regular_callable_replacement_factory()
class RegularCallableReplacementDecorated: ...

reveal_type(RegularCallableReplacementDecorated)  # revealed: Unknown
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

## Preserving the original class object

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

Class decorator factories that preserve the original class object also preserve the class binding:

```py
from collections.abc import Callable
from typing import Any, TypeVar, overload

DecoratorT = TypeVar("DecoratorT", bound=object)
DecoratedClass = type[DecoratorT]

@overload
def identity_class_decorator_factory(cls: DecoratedClass, **kwargs: Any) -> DecoratedClass: ...
@overload
def identity_class_decorator_factory(
    **kwargs: Any,
) -> Callable[[DecoratedClass], DecoratedClass]: ...
def identity_class_decorator_factory(
    cls: DecoratedClass | None = None, **kwargs: Any
) -> DecoratedClass | Callable[[DecoratedClass], DecoratedClass]:
    def decorator(inner_cls: DecoratedClass) -> DecoratedClass:
        return inner_cls

    if cls is not None:
        return decorator(cls)
    return decorator

@identity_class_decorator_factory(frozen=True)
class FactoryPreservedClass: ...

reveal_type(FactoryPreservedClass)  # revealed: <class 'FactoryPreservedClass'>

class DerivedFactoryPreservedClass(FactoryPreservedClass):
    value: FactoryPreservedClass
```

## Intersection-returning class decorators

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
    # error: [invalid-return-type] "Return type does not match returned value: expected `type[SchemaT@register] & ResourceEnabled`, found `type[SchemaT@register]`"
    return cls

@register
class UserSchema:
    id: int

reveal_type(UserSchema.resource.fetch())  # revealed: str
```

## Metadata decorators above intersection-returning decorators

Metadata decorators stacked above an intersection-returning class decorator still apply to the
original class object, while preserving the extra intersection members:

```py
from dataclasses import dataclass
from ty_extensions import Intersection
from typing import Protocol, TypeVar

class Resource:
    def fetch(self) -> str:
        return "data"

class ResourceEnabled(Protocol):
    resource: Resource

SchemaT = TypeVar("SchemaT")

def register(cls: type[SchemaT]) -> Intersection[type[SchemaT], ResourceEnabled]:
    # error: [invalid-return-type] "Return type does not match returned value: expected `type[SchemaT@register] & ResourceEnabled`, found `type[SchemaT@register]`"
    return cls

@dataclass
@register
class RegisteredDataclass:
    id: int

reveal_type(RegisteredDataclass.resource.fetch())  # revealed: str
reveal_type(RegisteredDataclass(1))  # revealed: RegisteredDataclass
```

## Class-preserving decorators above intersection-returning decorators

Class-preserving decorators stacked above an intersection-returning class decorator preserve the
existing intersection members:

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
    # error: [invalid-return-type] "Return type does not match returned value: expected `type[SchemaT@register] & ResourceEnabled`, found `type[SchemaT@register]`"
    return cls

def identity(cls: type[SchemaT]) -> type[SchemaT]:
    return cls

@identity
@register
class RegisteredIdentity:
    id: int

reveal_type(RegisteredIdentity.resource.fetch())  # revealed: str
```

## Dynamic function decorator returns

### Basics

A decorator that returns `Any` erases the original function's signature. Unannotated decorators have
the same effect because their `Unknown` return type is equivalent to `Any`. Our opt-in diagnostic
`dynamic-function-decorator-return` identifies and flags these cases, which will be undesirable for
users who want strict typing enforced on their codebases:

```py
from typing import Any, Callable

def returns_any(function: Callable[..., object]) -> Any:
    return function

# snapshot: dynamic-function-decorator-return
@returns_any
def fully_typed(value: int) -> str:
    return str(value)

reveal_type(fully_typed)  # revealed: Any
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Any`
 --> src/mdtest_snippet.py:7:1
  |
7 | @returns_any
  | ^^^^^^^^^^^^
8 | def fully_typed(value: int) -> str:
  |     ----------- Signature of `fully_typed` will be obscured by the decorator
  |
 ::: src/mdtest_snippet.py:3:5
  |
3 | def returns_any(function: Callable[..., object]) -> Any:
  |     --------------------------------------------------- `returns_any` defined here
```

### Dynamic decorators implemented by callable instances

A callable-instance decorator points to its `__call__` method and suggests adding a return
annotation when that method is unannotated.

```py
class CallableDecorator:
    def __call__(self, function: object):
        return function

# snapshot: dynamic-function-decorator-return
@CallableDecorator()
def decorated(value: int) -> str:
    return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Unknown`
 --> src/mdtest_snippet.py:6:1
  |
6 | @CallableDecorator()
  | ^^^^^^^^^^^^^^^^^^^^
7 | def decorated(value: int) -> str:
  |     --------- Signature of `decorated` will be obscured by the decorator
  |
 ::: src/mdtest_snippet.py:2:9
  |
2 |     def __call__(self, function: object):
  |         -------------------------------- `CallableDecorator.__call__` defined here
help: Add a return type annotation to `CallableDecorator.__call__`
```

### Dynamic decorators on overloaded function implementations

A dynamic decorator on an overload implementation does not obscure the externally visible overload
signatures, so it does not trigger `dynamic-function-decorator-return`:

```py
from collections.abc import Callable
from typing import Any, overload

def dynamic(function: Callable[..., object]) -> Any:
    return function

@overload
def decorated(value: int) -> int: ...
@overload
def decorated(value: str) -> str: ...
@dynamic
def decorated(value: int | str) -> int | str:
    return value

reveal_type(decorated)  # revealed: Overload[(value: int) -> int, (value: str) -> str]
reveal_type(decorated(1))  # revealed: int
reveal_type(decorated("hello"))  # revealed: str
```

### Subdiagnostics suggest adding annotations, where appropriate

Decorators imported from another first-party module point to their definition. If the diagnostic was
triggered due to a missing return-type annotation, we suggest adding one:

`decorator.py`:

```py
def dynamic(value: object):
    return value
```

`main.py`:

```py
from decorator import dynamic

# snapshot: dynamic-function-decorator-return
@dynamic
def decorated(value: int) -> str:
    return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Unknown`
 --> src/main.py:4:1
  |
4 | @dynamic
  | ^^^^^^^^
5 | def decorated(value: int) -> str:
  |     --------- Signature of `decorated` will be obscured by the decorator
  |
 ::: src/decorator.py:1:5
  |
1 | def dynamic(value: object):
  |     ---------------------- `dynamic` defined here
help: Add a return type annotation to `dynamic`
```

But we refrain from suggesting the user add a return annotation to the implementation of an
overloaded decorator function: the return annotation of the implementation is irrelevant to the
diagnostic in the following example:

```py
from typing import overload, Callable, Any

@overload
def overloaded_dynamic(function: Callable[..., object]) -> Any: ...
@overload
def overloaded_dynamic(function: None) -> None: ...
def overloaded_dynamic(function):
    return function

# snapshot: dynamic-function-decorator-return
@overloaded_dynamic
def decorated2(value: int) -> str:
    return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Any`
  --> src/mdtest_snippet.py:11:1
   |
11 | @overloaded_dynamic
   | ^^^^^^^^^^^^^^^^^^^
12 | def decorated2(value: int) -> str:
   |     ---------- Signature of `decorated2` will be obscured by the decorator
   |
  ::: src/mdtest_snippet.py:4:5
   |
 4 | def overloaded_dynamic(function: Callable[..., object]) -> Any: ...
   |     ---------------------------------------------------------- Matching overload defined here
```

### Dynamic decorators use the matched overload definition(s)

The definition annotation points to the overload selected by the implicit decorator call, even when
that overload is not the first declaration.

```py
from typing import overload

@overload
def dynamic(function, extra): ...
@overload
def dynamic(function): ...
def dynamic(function, extra=None):
    return function

# snapshot: dynamic-function-decorator-return
@dynamic
def decorated(value: int) -> str:
    return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Unknown`
  --> src/mdtest_snippet.py:11:1
   |
11 | @dynamic
   | ^^^^^^^^
12 | def decorated(value: int) -> str:
   |     --------- Signature of `decorated` will be obscured by the decorator
   |
  ::: src/mdtest_snippet.py:6:5
   |
 6 | def dynamic(function): ...
   |     ----------------- Matching overload defined here
help: Add a return type annotation to `dynamic`
```

When multiple overloads match, the definition annotation spans every overload:

```py
from collections.abc import Callable
from typing import Any, overload

@overload
def dynamic(function: Callable[[int], object]) -> Any: ...
@overload
def dynamic(function: None) -> None: ...
@overload
def dynamic(function: Callable[[str], object]): ...
def dynamic(function):
    return function

# snapshot: dynamic-function-decorator-return
@dynamic
def decorated(value: Any) -> object:
    return value
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Any`
  --> src/mdtest_snippet.py:27:1
   |
27 |   @dynamic
   |   ^^^^^^^^
28 |   def decorated(value: Any) -> object:
   |       --------- Signature of `decorated` will be obscured by the decorator
   |
  ::: src/mdtest_snippet.py:17:1
   |
17 | / @overload
18 | | def dynamic(function: Callable[[int], object]) -> Any: ...
19 | | @overload
20 | | def dynamic(function: None) -> None: ...
21 | | @overload
22 | | def dynamic(function: Callable[[str], object]): ...
   | |______________________________________________- Overloads of `dynamic` defined here
help: Ensure all `dynamic` overloads have a return annotation
```

### Fully annotated decorators with multiple matching overloads

Overload ambiguity can produce `Unknown` even when every overload already has a return annotation.
In that case, we do not suggest adding annotations that already exist.

```py
from collections.abc import Callable
from typing import Any, overload

@overload
def dynamic(function: Callable[[int], object]) -> int: ...
@overload
def dynamic(function: None) -> None: ...
@overload
def dynamic(function: Callable[[str], object]) -> str: ...
def dynamic(function):
    return function

# snapshot: dynamic-function-decorator-return
@dynamic
def decorated(value: Any) -> object:
    return value
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Unknown`
  --> src/mdtest_snippet.py:14:1
   |
14 |   @dynamic
   |   ^^^^^^^^
15 |   def decorated(value: Any) -> object:
   |       --------- Signature of `decorated` will be obscured by the decorator
   |
  ::: src/mdtest_snippet.py:4:1
   |
 4 | / @overload
 5 | | def dynamic(function: Callable[[int], object]) -> int: ...
 6 | | @overload
 7 | | def dynamic(function: None) -> None: ...
 8 | | @overload
 9 | | def dynamic(function: Callable[[str], object]) -> str: ...
   | |_____________________________________________________- Overloads of `dynamic` defined here
```

### Dynamic decorators defined in third-party packages

A decorator from a dependency still points to its definition, but ty does not suggest editing code
outside the current project.

```toml
[environment]
python = "/.venv"
```

`/.venv/<path-to-site-packages>/dependency.py`:

```py
def dynamic(value: object):
    return value
```

`main.py`:

```py
from dependency import dynamic

# snapshot: dynamic-function-decorator-return
@dynamic
def decorated(value: int) -> str:
    return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Unknown`
 --> src/main.py:4:1
  |
4 | @dynamic
  | ^^^^^^^^
5 | def decorated(value: int) -> str:
  |     --------- Signature of `decorated` will be obscured by the decorator
  |
 ::: .venv/<path-to-site-packages>/dependency.py:1:5
  |
1 | def dynamic(value: object):
  |     ---------------------- `dynamic` defined here
```

### Edge case: dynamic decorators defined in non-module scripts

Decorators defined in the checked file receive a suggestion to add a return-type annotation even
when the filename is not a valid Python module name. (`typed-script.py` cannot be resolved to a
valid Python module by our module resolver, so cannot be recognised as having a "first-party search
path", but we nonetheless recognise it as a first-party file and offer the suggestion.)

`typed-script.py`:

```py
def dynamic(function: object):
    return function

# snapshot: dynamic-function-decorator-return
@dynamic
def decorated(value: int) -> str:
    return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Unknown`
 --> src/typed-script.py:5:1
  |
5 | @dynamic
  | ^^^^^^^^
6 | def decorated(value: int) -> str:
  |     --------- Signature of `decorated` will be obscured by the decorator
  |
 ::: src/typed-script.py:1:5
  |
1 | def dynamic(function: object):
  |     ------------------------- `dynamic` defined here
help: Add a return type annotation to `dynamic`
```

### Edge case: the decorator has a union type

This comes up very rarely, so for simplicity's sake we just don't have any secondary annotations
here:

```py
from collections.abc import Callable
from typing import Any

def annotated_dynamic(function: Callable[..., object]) -> Any:
    return function

def unannotated_dynamic(function: Callable[..., object]):
    return function

def condition() -> bool:
    return True

decorator = annotated_dynamic if condition() else unannotated_dynamic

# snapshot: dynamic-function-decorator-return
@decorator
def decorated(value: int) -> str:
    return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Any`
  --> src/mdtest_snippet.py:16:1
   |
16 | @decorator
   | ^^^^^^^^^^
17 | def decorated(value: int) -> str:
   |     --------- Signature of `decorated` will be obscured by the decorator
```

### Edge case: the decorator is a union of an overloaded function and `Callable`

When one union member is an overloaded function and another is a `Callable`, the latter's bindings
must not be attributed to the overloaded function.

```py
from typing import Any, Callable, overload

@overload
def overloaded_dynamic(value: None): ...
@overload
def overloaded_dynamic(value: Callable[[int], str]) -> Any: ...
def overloaded_dynamic(value: object) -> Any:
    return value

def apply_decorator(flag: bool, other: Callable[[Callable[..., object]], Any]) -> None:
    decorator = overloaded_dynamic if flag else other

    # snapshot: dynamic-function-decorator-return
    @decorator
    def decorated(value: int) -> str:
        return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Any`
  --> src/mdtest_snippet.py:14:5
   |
14 |     @decorator
   |     ^^^^^^^^^^
15 |     def decorated(value: int) -> str:
   |         --------- Signature of `decorated` will be obscured by the decorator
```

### Multiple dynamic decorators

Only the first decorator that replaces a precise type with a dynamic type is reported. Outer
decorators receive an already-dynamic type, so they do not lose any additional information.

```py
from typing import Any

def dynamic(value: Any) -> Any:
    return value

# no error for this outer decorator (the received type was already `Any`)
@dynamic
# error: [dynamic-function-decorator-return]
@dynamic
def decorated_function(value: int) -> str:
    return str(value)
```

### Dynamic decorators applied to replacement values

When an inner decorator replaces a function with a non-callable value, an outer dynamic decorator
obscures that replacement value's type rather than the original function signature.

```py
from collections.abc import Callable
from typing import Any

def replace_with_int(function: Callable[..., object]) -> int:
    return 1

def dynamic(value: int) -> Any:
    return value

# snapshot: dynamic-function-decorator-return
@dynamic
@replace_with_int
def decorated(value: int) -> str:
    return str(value)
```

```snapshot
info[dynamic-function-decorator-return]: Decorator returns `Any`
  --> src/mdtest_snippet.py:11:1
   |
11 | @dynamic
   | ^^^^^^^^
12 | @replace_with_int
13 | def decorated(value: int) -> str:
   |     --------- Previous type of `decorated` will be obscured by the decorator
   |
  ::: src/mdtest_snippet.py:7:5
   |
 7 | def dynamic(value: int) -> Any:
   |     -------------------------- `dynamic` defined here
```

### Decorator return types equivalent to `Any`

Aliases of `Any` erase the decorated type in the same way as a direct `Any` annotation.

```py
from typing import Any, TypeAlias

DynamicAlias: TypeAlias = Any

def returns_alias(value: object) -> DynamicAlias:
    return value

# error: [dynamic-function-decorator-return]
@returns_alias
def decorated_function(value: int) -> str:
    return str(value)
```

### Partially dynamic decorator returns

A decorator does not erase all information when its return type merely contains `Any`. Such a type
is not equivalent to `Any`, so the decorator is not reported.

```py
from collections.abc import Callable
from typing import Any

def returns_callable(function: Callable[..., object]) -> Callable[..., Any]:
    return function

@returns_callable
def decorated_function(value: int) -> str:
    return str(value)
```
