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

# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 1, got 2"
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

reveal_type(MyClass)  # revealed: <class 'MyClass'>
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
    # error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 1, got 2"
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

# TODO: the revealed type should ideally be `type[int]` (the decorator's return type)
reveal_type(Baz)  # revealed: <class 'Baz'>
```

## Decorated overloaded functions

Decorators using `ParamSpec` and `TypeVar` should preserve overload return types.

### Basic ParamSpec decorator

```py
from typing import overload, ParamSpec, TypeVar, Callable

P = ParamSpec("P")
T = TypeVar("T")

def decorator(f: Callable[P, T]) -> Callable[P, T]:
    return f

@overload
def test(x: int) -> int: ...
@overload
def test(x: str) -> str: ...
@decorator
def test(x: int | str) -> int | str:
    return x

# revealed: Overload[(x: int) -> int, (x: str) -> str]
reveal_type(test)

# The decorated function should preserve overload return types
reveal_type(test(1))  # revealed: int
reveal_type(test("hello"))  # revealed: str
```

### ParamSpec decorator with keyword-only overloads

```py
from typing import overload, ParamSpec, TypeVar, Callable

P = ParamSpec("P")
T = TypeVar("T")

def decorator(f: Callable[P, T]) -> Callable[P, T]:
    return f

@overload
def test(x: int) -> int: ...
@overload
def test(*, y: str) -> str: ...
@decorator
def test(x: int | None = None, *, y: str | None = None) -> int | str:
    raise NotImplementedError

# revealed: Overload[(x: int) -> int, (*, y: str) -> str]
reveal_type(test)

reveal_type(test(1))  # revealed: int
reveal_type(test(y="hello"))  # revealed: str
```

### Multiple decorators with ParamSpec

```py
from typing import overload, ParamSpec, TypeVar, Callable

P = ParamSpec("P")
T = TypeVar("T")

def decorator1(f: Callable[P, T]) -> Callable[P, T]:
    return f

def decorator2(f: Callable[P, T]) -> Callable[P, T]:
    return f

@overload
def test(x: int) -> int: ...
@overload
def test(x: str) -> str: ...
@decorator1
@decorator2
def test(x: int | str) -> int | str:
    return x

# revealed: Overload[(x: int) -> int, (x: str) -> str]
reveal_type(test)

reveal_type(test(1))  # revealed: int
reveal_type(test("hello"))  # revealed: str
```

### `functools.wraps` with overloads

```py
from typing import overload, ParamSpec, TypeVar, Callable
import functools

P = ParamSpec("P")
T = TypeVar("T")

def decorator(f: Callable[P, T]) -> Callable[P, T]:
    @functools.wraps(f)
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> T:
        return f(*args, **kwargs)
    return wrapper

@overload
def test(x: int) -> int: ...
@overload
def test(x: str) -> str: ...
@decorator
def test(x: int | str) -> int | str:
    return x

reveal_type(test(1))  # revealed: int
reveal_type(test("hello"))  # revealed: str
```

### Method overloads with decorator

```py
from typing import overload, ParamSpec, TypeVar, Callable

P = ParamSpec("P")
T = TypeVar("T")

def decorator(f: Callable[P, T]) -> Callable[P, T]:
    return f

class MyClass:
    @overload
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...
    @decorator
    def method(self, x: int | str) -> int | str:
        return x

obj = MyClass()
reveal_type(obj.method(1))  # revealed: int
reveal_type(obj.method("hello"))  # revealed: str
```

### Non-ParamSpec decorator (should use implementation return type)

When a decorator doesn't use ParamSpec, it cannot preserve overload signatures, so the
implementation's return type is used:

```py
from typing import overload, Callable

def simple_decorator(f: Callable[..., object]) -> Callable[..., object]:
    return f

@overload
def test(x: int) -> int: ...
@overload
def test(x: str) -> str: ...
@simple_decorator
def test(x: int | str) -> int | str:
    return x

# Without ParamSpec, overloads are not preserved
reveal_type(test(1))  # revealed: object
```

### Decorator that transforms return type

When a decorator transforms the return type (e.g., `T` to `list[T]`), the transformed type should be
applied to each overload's return type:

```py
from typing import overload, ParamSpec, TypeVar, Callable

P = ParamSpec("P")
T = TypeVar("T")

def wrap_in_list(f: Callable[P, T]) -> Callable[P, list[T]]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> list[T]:
        return [f(*args, **kwargs)]
    return wrapper

@overload
def test(x: int) -> int: ...
@overload
def test(x: str) -> str: ...
@wrap_in_list
def test(x: int | str) -> int | str:
    return x

reveal_type(test(1))  # revealed: list[int]
reveal_type(test("hello"))  # revealed: list[str]
```

### Decorator with multiple type variables

When a decorator has multiple type variables, we should correctly identify and substitute the one
that corresponds to the return type:

```py
from typing import overload, ParamSpec, TypeVar, Callable

P = ParamSpec("P")
T = TypeVar("T")
U = TypeVar("U")

# T is used multiple times in the return type
def double_wrap(f: Callable[P, T]) -> Callable[P, tuple[T, T]]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> tuple[T, T]:
        result = f(*args, **kwargs)
        return (result, result)
    return wrapper

@overload
def test1(x: int) -> int: ...
@overload
def test1(x: str) -> str: ...
@double_wrap
def test1(x: int | str) -> int | str:
    return x

reveal_type(test1(1))  # revealed: tuple[int, int]
reveal_type(test1("hello"))  # revealed: tuple[str, str]

# Multiple TypeVars where only one is related to overload return types
def with_default(f: Callable[P, T], default: U) -> Callable[P, T | U]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> T | U:
        try:
            return f(*args, **kwargs)
        except Exception:
            raise  # Just for type checking
    return wrapper

@overload
def test2(x: int) -> int: ...
@overload
def test2(x: str) -> str: ...
def test2(x: int | str) -> int | str:
    return x

# When decorating with a specific default type, U is bound to that type
# T should still preserve per-overload return types
wrapped = with_default(test2, None)
reveal_type(wrapped(1))  # revealed: int | None
reveal_type(wrapped("hello"))  # revealed: str | None
```
