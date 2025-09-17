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

reveal_type(get_int_async())  # revealed: CoroutineType[Any, Any, int]
```

## Generic

```toml
[environment]
python-version = "3.12"
```

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

## PEP-484 convention for positional-only parameters

PEP 570, introduced in Python 3.8, added dedicated Python syntax for denoting positional-only
parameters (the `/` in a function signature). However, functions implemented in C were able to have
positional-only parameters prior to Python 3.8 (there was just no syntax for expressing this at the
Python level).

Stub files describing functions implemented in C nonetheless needed a way of expressing that certain
parameters were positional-only. In the absence of dedicated Python syntax, PEP 484 described a
convention that type checkers were expected to understand:

> Some functions are designed to take their arguments only positionally, and expect their callers
> never to use the argument’s name to provide that argument by keyword. All arguments with names
> beginning with `__` are assumed to be positional-only, except if their names also end with `__`.

While this convention is now redundant (following the implementation of PEP 570), many projects
still continue to use the old convention, so it is supported by ty as well.

```py
def f(__x: int): ...

f(1)
# error: [missing-argument]
# error: [unknown-argument]
f(__x=1)
```

But not if they follow a non-positional-only parameter:

```py
def g(x: int, __y: str): ...

g(x=1, __y="foo")
```

And also not if they both start and end with `__`:

```py
def h(__x__: str): ...

h(__x__="foo")
```

And if *any* parameters use the new PEP-570 convention, the old convention does not apply:

```py
def i(x: str, /, __y: int): ...

i("foo", __y=42)  # fine
```

And `self`/`cls` are implicitly positional-only:

```py
class C:
    def method(self, __x: int): ...
    @classmethod
    def class_method(cls, __x: str): ...
    # (the name of the first parameter is irrelevant;
    # a staticmethod works the same as a free function in the global scope)
    @staticmethod
    def static_method(self, __x: int): ...

# error: [missing-argument]
# error: [unknown-argument]
C().method(__x=1)
# error: [missing-argument]
# error: [unknown-argument]
C.class_method(__x="1")
C.static_method("x", __x=42)  # fine
```

## Splatted arguments

### Unknown argument length

```py
def takes_zero() -> None: ...
def takes_one(x: int) -> None: ...
def takes_two(x: int, y: int) -> None: ...
def takes_two_positional_only(x: int, y: int, /) -> None: ...
def takes_two_different(x: int, y: str) -> None: ...
def takes_two_different_positional_only(x: int, y: str, /) -> None: ...
def takes_at_least_zero(*args) -> None: ...
def takes_at_least_one(x: int, *args) -> None: ...
def takes_at_least_two(x: int, y: int, *args) -> None: ...
def takes_at_least_two_positional_only(x: int, y: int, /, *args) -> None: ...

# Test all of the above with a number of different splatted argument types

def _(args: list[int]) -> None:
    takes_zero(*args)
    takes_one(*args)
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

def _(args: tuple[int, ...]) -> None:
    takes_zero(*args)
    takes_one(*args)
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)
```

### Fixed-length tuple argument

```py
def takes_zero() -> None: ...
def takes_one(x: int) -> None: ...
def takes_two(x: int, y: int) -> None: ...
def takes_two_positional_only(x: int, y: int, /) -> None: ...
def takes_two_different(x: int, y: str) -> None: ...
def takes_two_different_positional_only(x: int, y: str, /) -> None: ...
def takes_at_least_zero(*args) -> None: ...
def takes_at_least_one(x: int, *args) -> None: ...
def takes_at_least_two(x: int, y: int, *args) -> None: ...
def takes_at_least_two_positional_only(x: int, y: int, /, *args) -> None: ...

# Test all of the above with a number of different splatted argument types

def _(args: tuple[int]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)
    takes_two(*args)  # error: [missing-argument]
    takes_two_positional_only(*args)  # error: [missing-argument]
    takes_two_different(*args)  # error: [missing-argument]
    takes_two_different_positional_only(*args)  # error: [missing-argument]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)  # error: [missing-argument]
    takes_at_least_two_positional_only(*args)  # error: [missing-argument]

def _(args: tuple[int, int]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

def _(args: tuple[int, str]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)  # error: [invalid-argument-type]
    takes_two_positional_only(*args)  # error: [invalid-argument-type]
    takes_two_different(*args)
    takes_two_different_positional_only(*args)
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)  # error: [invalid-argument-type]
    takes_at_least_two_positional_only(*args)  # error: [invalid-argument-type]
```

### Subclass of fixed-length tuple argument

```py
def takes_zero() -> None: ...
def takes_one(x: int) -> None: ...
def takes_two(x: int, y: int) -> None: ...
def takes_two_positional_only(x: int, y: int, /) -> None: ...
def takes_two_different(x: int, y: str) -> None: ...
def takes_two_different_positional_only(x: int, y: str, /) -> None: ...
def takes_at_least_zero(*args) -> None: ...
def takes_at_least_one(x: int, *args) -> None: ...
def takes_at_least_two(x: int, y: int, *args) -> None: ...
def takes_at_least_two_positional_only(x: int, y: int, /, *args) -> None: ...

# Test all of the above with a number of different splatted argument types

class SingleElementTuple(tuple[int]): ...

def _(args: SingleElementTuple) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]

    takes_one(*args)

    takes_two(*args)  # error: [missing-argument]
    takes_two_positional_only(*args)  # error: [missing-argument]

    takes_two_different(*args)  # error: [missing-argument]
    takes_two_different_positional_only(*args)  # error: [missing-argument]

    takes_at_least_zero(*args)
    takes_at_least_one(*args)

    takes_at_least_two(*args)  # error: [missing-argument]
    takes_at_least_two_positional_only(*args)  # error: [missing-argument]

class TwoElementIntTuple(tuple[int, int]): ...

def _(args: TwoElementIntTuple) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

class IntStrTuple(tuple[int, str]): ...

def _(args: IntStrTuple) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]

    takes_one(*args)  # error: [too-many-positional-arguments]

    # error: [invalid-argument-type]
    takes_two(*args)
    # error: [invalid-argument-type]
    takes_two_positional_only(*args)

    takes_two_different(*args)
    takes_two_different_positional_only(*args)
    takes_at_least_zero(*args)
    takes_at_least_one(*args)

    # error: [invalid-argument-type]
    takes_at_least_two(*args)
    # error: [invalid-argument-type]
    takes_at_least_two_positional_only(*args)
```

### Mixed tuple argument

```toml
[environment]
python-version = "3.11"
```

```py
def takes_zero() -> None: ...
def takes_one(x: int) -> None: ...
def takes_two(x: int, y: int) -> None: ...
def takes_two_positional_only(x: int, y: int, /) -> None: ...
def takes_two_different(x: int, y: str) -> None: ...
def takes_two_different_positional_only(x: int, y: str, /) -> None: ...
def takes_at_least_zero(*args) -> None: ...
def takes_at_least_one(x: int, *args) -> None: ...
def takes_at_least_two(x: int, y: int, *args) -> None: ...
def takes_at_least_two_positional_only(x: int, y: int, /, *args) -> None: ...

# Test all of the above with a number of different splatted argument types

def _(args: tuple[int, *tuple[int, ...]]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

def _(args: tuple[int, *tuple[str, ...]]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)
    takes_two(*args)  # error: [invalid-argument-type]
    takes_two_positional_only(*args)  # error: [invalid-argument-type]
    takes_two_different(*args)
    takes_two_different_positional_only(*args)
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)  # error: [invalid-argument-type]
    takes_at_least_two_positional_only(*args)  # error: [invalid-argument-type]

def _(args: tuple[int, int, *tuple[int, ...]]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

def _(args: tuple[int, int, *tuple[str, ...]]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

def _(args: tuple[int, *tuple[int, ...], int]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

def _(args: tuple[int, *tuple[str, ...], int]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)  # error: [invalid-argument-type]
    takes_two_positional_only(*args)  # error: [invalid-argument-type]
    takes_two_different(*args)
    takes_two_different_positional_only(*args)
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)  # error: [invalid-argument-type]
    takes_at_least_two_positional_only(*args)  # error: [invalid-argument-type]
```

### Subclass of mixed tuple argument

```toml
[environment]
python-version = "3.11"
```

```py
def takes_zero() -> None: ...
def takes_one(x: int) -> None: ...
def takes_two(x: int, y: int) -> None: ...
def takes_two_positional_only(x: int, y: int, /) -> None: ...
def takes_two_different(x: int, y: str) -> None: ...
def takes_two_different_positional_only(x: int, y: str, /) -> None: ...
def takes_at_least_zero(*args) -> None: ...
def takes_at_least_one(x: int, *args) -> None: ...
def takes_at_least_two(x: int, y: int, *args) -> None: ...
def takes_at_least_two_positional_only(x: int, y: int, /, *args) -> None: ...

# Test all of the above with a number of different splatted argument types

class IntStarInt(tuple[int, *tuple[int, ...]]): ...

def _(args: IntStarInt) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

class IntStarStr(tuple[int, *tuple[str, ...]]): ...

def _(args: IntStarStr) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]

    takes_one(*args)

    # error: [invalid-argument-type]
    takes_two(*args)
    # error: [invalid-argument-type]
    takes_two_positional_only(*args)

    takes_two_different(*args)
    takes_two_different_positional_only(*args)

    takes_at_least_zero(*args)

    takes_at_least_one(*args)

    # error: [invalid-argument-type]
    takes_at_least_two(*args)
    # error: [invalid-argument-type]
    takes_at_least_two_positional_only(*args)

class IntIntStarInt(tuple[int, int, *tuple[int, ...]]): ...

def _(args: IntIntStarInt) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

class IntIntStarStr(tuple[int, int, *tuple[str, ...]]): ...

def _(args: IntIntStarStr) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]

    takes_one(*args)  # error: [too-many-positional-arguments]

    takes_two(*args)
    takes_two_positional_only(*args)

    # error: [invalid-argument-type]
    takes_two_different(*args)
    # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)

    takes_at_least_zero(*args)

    takes_at_least_one(*args)

    takes_at_least_two(*args)

    takes_at_least_two_positional_only(*args)

class IntStarIntInt(tuple[int, *tuple[int, ...], int]): ...

def _(args: IntStarIntInt) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

class IntStarStrInt(tuple[int, *tuple[str, ...], int]): ...

def _(args: IntStarStrInt) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]

    takes_one(*args)  # error: [too-many-positional-arguments]

    # error: [invalid-argument-type]
    takes_two(*args)
    # error: [invalid-argument-type]
    takes_two_positional_only(*args)

    takes_two_different(*args)
    takes_two_different_positional_only(*args)

    takes_at_least_zero(*args)

    takes_at_least_one(*args)

    # error: [invalid-argument-type]
    takes_at_least_two(*args)

    # error: [invalid-argument-type]
    takes_at_least_two_positional_only(*args)
```

### String argument

```py
from typing import Literal

def takes_zero() -> None: ...
def takes_one(x: str) -> None: ...
def takes_two(x: str, y: str) -> None: ...
def takes_two_positional_only(x: str, y: str, /) -> None: ...
def takes_two_different(x: int, y: str) -> None: ...
def takes_two_different_positional_only(x: int, y: str, /) -> None: ...
def takes_at_least_zero(*args) -> None: ...
def takes_at_least_one(x: str, *args) -> None: ...
def takes_at_least_two(x: str, y: str, *args) -> None: ...
def takes_at_least_two_positional_only(x: str, y: str, /, *args) -> None: ...

# Test all of the above with a number of different splatted argument types

def _(args: Literal["a"]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)
    takes_two(*args)  # error: [missing-argument]
    takes_two_positional_only(*args)  # error: [missing-argument]
    # error: [invalid-argument-type]
    # error: [missing-argument]
    takes_two_different(*args)
    # error: [invalid-argument-type]
    # error: [missing-argument]
    takes_two_different_positional_only(*args)
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)  # error: [missing-argument]
    takes_at_least_two_positional_only(*args)  # error: [missing-argument]

def _(args: Literal["ab"]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

def _(args: Literal["abc"]) -> None:
    takes_zero(*args)  # error: [too-many-positional-arguments]
    takes_one(*args)  # error: [too-many-positional-arguments]
    takes_two(*args)  # error: [too-many-positional-arguments]
    takes_two_positional_only(*args)  # error: [too-many-positional-arguments]
    # error: [invalid-argument-type]
    # error: [too-many-positional-arguments]
    takes_two_different(*args)
    # error: [invalid-argument-type]
    # error: [too-many-positional-arguments]
    takes_two_different_positional_only(*args)
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

def _(args: str) -> None:
    takes_zero(*args)
    takes_one(*args)
    takes_two(*args)
    takes_two_positional_only(*args)
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]
    takes_at_least_zero(*args)
    takes_at_least_one(*args)
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)
```

## Wrong argument type

### Positional argument, positional-or-keyword parameter

```py
def f(x: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["foo"]`"
reveal_type(f("foo"))  # revealed: int
```

### Positional argument, positional-only parameter

```py
def f(x: int, /) -> int:
    return 1

# error: 15 [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["foo"]`"
reveal_type(f("foo"))  # revealed: int
```

### Positional argument, variadic parameter

```py
def f(*args: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["foo"]`"
reveal_type(f("foo"))  # revealed: int
```

### Keyword argument, positional-or-keyword parameter

```py
def f(x: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["foo"]`"
reveal_type(f(x="foo"))  # revealed: int
```

### Keyword argument, keyword-only parameter

```py
def f(*, x: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["foo"]`"
reveal_type(f(x="foo"))  # revealed: int
```

### Keyword argument, keywords parameter

```py
def f(**kwargs: int) -> int:
    return 1

# error: 15 [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["foo"]`"
reveal_type(f(x="foo"))  # revealed: int
```

### Correctly match keyword out-of-order

```py
def f(x: int = 1, y: str = "foo") -> int:
    return 1

# error: 15 [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `Literal[2]`"
# error: 20 [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `Literal["bar"]`"
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
from ty_extensions import static_assert

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

### Type property predicates

```py
from ty_extensions import is_subtype_of

# error: [missing-argument]
is_subtype_of()

# error: [missing-argument]
is_subtype_of(int)

# error: [too-many-positional-arguments]
is_subtype_of(int, int, int)

# error: [too-many-positional-arguments]
is_subtype_of(int, int, int, int)
```
