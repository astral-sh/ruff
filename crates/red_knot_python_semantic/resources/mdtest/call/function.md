# Call expression

## Simple

```py
def get_int() -> int:
    return 42

reveal_type(get_int())  # revealed: int
```

## Async

```py
async def get_int_async() -> int:
    return 42

# TODO: we don't yet support `types.CoroutineType`, should be generic `Coroutine[Any, Any, int]`
reveal_type(get_int_async())  # revealed: @Todo(generic types.CoroutineType)
```

## Generic

```py
def get_int[T]() -> int:
    return 42

reveal_type(get_int())  # revealed: int
```

## Decorated

```py
from typing import Callable

def foo() -> int:
    return 42

def decorator(func) -> Callable[[], int]:
    return foo

@decorator
def bar() -> str:
    return "bar"

reveal_type(bar())  # revealed: int
```

## Invalid callable

```py
nonsense = 123
x = nonsense()  # error: "Object of type `Literal[123]` is not callable"
```

## Potentially unbound function

```py
def _(flag: bool):
    if flag:
        def foo() -> int:
            return 42
    # error: [possibly-unresolved-reference]
    reveal_type(foo())  # revealed: int
```

## Wrong argument type

### Positional argument, positional-or-keyword parameter

```py
def f(x: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to parameter 1 (`x`) of function `f`; expected type `int`"
reveal_type(f("foo"))  # revealed: int
```

### Positional argument, positional-only parameter

```py
def f(x: int, /) -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to parameter 1 (`x`) of function `f`; expected type `int`"
reveal_type(f("foo"))  # revealed: int
```

### Positional argument, variadic parameter

```py
def f(*args: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to parameter `*args` of function `f`; expected type `int`"
reveal_type(f("foo"))  # revealed: int
```

### Keyword argument, positional-or-keyword parameter

```py
def f(x: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to parameter `x` of function `f`; expected type `int`"
reveal_type(f(x="foo"))  # revealed: int
```

### Keyword argument, keyword-only parameter

```py
def f(*, x: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to parameter `x` of function `f`; expected type `int`"
reveal_type(f(x="foo"))  # revealed: int
```

### Keyword argument, keywords parameter

```py
def f(**kwargs: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to parameter `**kwargs` of function `f`; expected type `int`"
reveal_type(f(x="foo"))  # revealed: int
```

### Correctly match keyword out-of-order

```py
def f(x: int = 1, y: str = "foo") -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal[2]` cannot be assigned to parameter `y` of function `f`; expected type `str`"
# error: 20 [invalid-argument-type] "Object of type `Literal["bar"]` cannot be assigned to parameter `x` of function `f`; expected type `int`"
reveal_type(f(y=2, x="bar"))  # revealed: int
```

## Too many positional arguments

### One too many

```py
def f() -> int:
    return 1

# error: 15 [too-many-positional-arguments] "Too many positional arguments to function `f`: expected 0, got 1"
reveal_type(f("foo"))  # revealed: int
```

### Two too many

```py
def f() -> int:
    return 1

# error: 15 [too-many-positional-arguments] "Too many positional arguments to function `f`: expected 0, got 2"
reveal_type(f("foo", "bar"))  # revealed: int
```

### No too-many-positional if variadic is taken

```py
def f(*args: int) -> int:
    return 1

reveal_type(f(1, 2, 3))  # revealed: int
```

### Multiple keyword arguments map to keyword variadic parameter

```py
def f(**kwargs: int) -> int:
    return 1

reveal_type(f(foo=1, bar=2))  # revealed: int
```

## Missing arguments

### No defaults or variadic

```py
def f(x: int) -> int:
    return 1

# error: 13 [missing-argument] "No argument provided for required parameter `x` of function `f`"
reveal_type(f())  # revealed: int
```

### With default

```py
def f(x: int, y: str = "foo") -> int:
    return 1

# error: 13 [missing-argument] "No argument provided for required parameter `x` of function `f`"
reveal_type(f())  # revealed: int
```

### Defaulted argument is not required

```py
def f(x: int = 1) -> int:
    return 1

reveal_type(f())  # revealed: int
```

### With variadic

```py
def f(x: int, *y: str) -> int:
    return 1

# error: 13 [missing-argument] "No argument provided for required parameter `x` of function `f`"
reveal_type(f())  # revealed: int
```

### Variadic argument is not required

```py
def f(*args: int) -> int:
    return 1

reveal_type(f())  # revealed: int
```

### Keywords argument is not required

```py
def f(**kwargs: int) -> int:
    return 1

reveal_type(f())  # revealed: int
```

### Multiple

```py
def f(x: int, y: int) -> int:
    return 1

# error: 13 [missing-argument] "No arguments provided for required parameters `x`, `y` of function `f`"
reveal_type(f())  # revealed: int
```

## Unknown argument

```py
def f(x: int) -> int:
    return 1

# error: 20 [unknown-argument] "Argument `y` does not match any known parameter of function `f`"
reveal_type(f(x=1, y=2))  # revealed: int
```

## Parameter already assigned

```py
def f(x: int) -> int:
    return 1

# error: 18 [parameter-already-assigned] "Multiple values provided for parameter `x` of function `f`"
reveal_type(f(1, x=2))  # revealed: int
```

## Special functions

Some functions require special handling in type inference. Here, we make sure that we still emit
proper diagnostics in case of missing or superfluous arguments.

### `reveal_type`

```py
from typing_extensions import reveal_type

# error: [missing-argument] "No argument provided for required parameter `obj` of function `reveal_type`"
reveal_type()

# error: [too-many-positional-arguments] "Too many positional arguments to function `reveal_type`: expected 1, got 2"
reveal_type(1, 2)
```

### `static_assert`

```py
from knot_extensions import static_assert

# error: [missing-argument] "No argument provided for required parameter `condition` of function `static_assert`"
static_assert()

# error: [too-many-positional-arguments] "Too many positional arguments to function `static_assert`: expected 2, got 3"
static_assert(True, 2, 3)
```

### `len`

```py
# error: [missing-argument] "No argument provided for required parameter `obj` of function `len`"
len()

# error: [too-many-positional-arguments] "Too many positional arguments to function `len`: expected 1, got 2"
len([], 1)
```

### Type API predicates

```py
from knot_extensions import is_subtype_of, is_fully_static

# error: [missing-argument]
is_subtype_of()

# error: [missing-argument]
is_subtype_of(int)

# error: [too-many-positional-arguments]
is_subtype_of(int, int, int)

# error: [too-many-positional-arguments]
is_subtype_of(int, int, int, int)

# error: [missing-argument]
is_fully_static()

# error: [too-many-positional-arguments]
is_fully_static(int, int)
```
