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

# TODO: should reveal `int`, as the decorator replaces `bar` with `foo`
reveal_type(bar())  # revealed: @Todo(return type)
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

# error: 15 [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to parameter `x` of function `f`; expected type `int`"
reveal_type(f("foo"))  # revealed: int
```

### Positional argument, positional-only parameter

```py
def f(x: int, /) -> int:
    return 1

# error: 15 [invalid-argument-type] "Object of type `Literal["foo"]` cannot be assigned to positional parameter 0 (`x`) of function `f`; expected type `int`"
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

# error: 15 [too-many-positional-arguments] "Too many positional arguments: expected 0, got 1"
reveal_type(f("foo"))  # revealed: int
```

### Two too many

```py
def f() -> int:
    return 1

# error: 15 [too-many-positional-arguments] "Too many positional arguments: expected 0, got 2"
reveal_type(f("foo", "bar"))  # revealed: int
```

### No too-many-positional if variadic is taken

```py
def f(*args: int) -> int:
    return 1

reveal_type(f(1, 2, 3))  # revealed: int
```

## Missing arguments

### No defaults or variadic

```py
def f(x: int) -> int:
    return 1

# error: 13 [missing-argument] "No argument provided for required parameter `x`"
reveal_type(f())  # revealed: int
```

### With default

```py
def f(x: int, y: str = "foo") -> int:
    return 1

# error: 13 [missing-argument] "No argument provided for required parameter `x`"
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

# error: 13 [missing-argument] "No argument provided for required parameter `x`"
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

## Unknown argument

```py
def f(x: int) -> int:
    return 1

# error: 20 [unknown-argument] "Argument `y` does not match any known parameter"
reveal_type(f(x=1, y=2))  # revealed: int
```

## Parameter already assigned

```py
def f(x: int) -> int:
    return 1

# error: 18 [parameter-already-assigned] "Got multiple values for parameter `x`"
reveal_type(f(1, x=2))  # revealed: int
```
