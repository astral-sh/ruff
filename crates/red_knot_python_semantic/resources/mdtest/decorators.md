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

# TODO: Should be `_lru_cache_wrapper[int]`
reveal_type(f)  # revealed: @Todo(generics)

# TODO: Should be `int`
reveal_type(f(1))  # revealed: @Todo(generics)
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

# error: [invalid-argument-type] "Object of type `Literal[f]` cannot be assigned to parameter 1 (`f`) of function `wrong_signature`; expected type `int`"
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
