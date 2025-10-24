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
# error: [positional-only-parameter-as-kwarg]
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

# error: [positional-only-parameter-as-kwarg]
C().method(__x=1)
# error: [positional-only-parameter-as-kwarg]
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
    takes_two(*b"ab")
    takes_two(*b"abc")  # error: [too-many-positional-arguments]
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

### Variadic argument, variadic parameter

```toml
[environment]
python-version = "3.11"
```

```py
def f(*args: int) -> int:
    return 1

def _(args: list[str]) -> None:
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `str`"
    reveal_type(f(*args))  # revealed: int
```

Considering a few different shapes of tuple for the splatted argument:

```py
def f1(*args: str): ...
def _(
    args1: tuple[str, ...],
    args2: tuple[str, *tuple[str, ...]],
    args3: tuple[str, *tuple[str, ...], str],
    args4: tuple[int, *tuple[str, ...]],
    args5: tuple[int, *tuple[str, ...], str],
    args6: tuple[*tuple[str, ...], str],
    args7: tuple[*tuple[str, ...], int],
    args8: tuple[int, *tuple[str, ...], int],
    args9: tuple[str, *tuple[str, ...], int],
    args10: tuple[str, *tuple[int, ...], str],
):
    f1(*args1)
    f1(*args2)
    f1(*args3)
    f1(*args4)  # error: [invalid-argument-type]
    f1(*args5)  # error: [invalid-argument-type]
    f1(*args6)
    f1(*args7)  # error: [invalid-argument-type]

    # The reason for two errors here is because of the two fixed elements in the tuple of `args8`
    # which are both `int`
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    f1(*args8)

    f1(*args9)  # error: [invalid-argument-type]
    f1(*args10)  # error: [invalid-argument-type]
```

A union of heterogeneous tuples provided to a variadic parameter:

```py
# Test inspired by ecosystem code at:
# - <https://github.com/home-assistant/core/blob/bde4eb50111a72f9717fe73ee5929e50eb06911b/homeassistant/components/lovelace/websocket.py#L50-L59>
# - <https://github.com/pydata/xarray/blob/3572f4e70f2b12ef9935c1f8c3c1b74045d2a092/xarray/tests/test_groupby.py#L3058-L3059>

def f2(a: str, b: bool): ...
def f3(coinflip: bool):
    if coinflip:
        args = "foo", True
    else:
        args = "bar", False

    # revealed: tuple[Literal["foo"], Literal[True]] | tuple[Literal["bar"], Literal[False]]
    reveal_type(args)
    f2(*args)  # fine

    if coinflip:
        other_args = "foo", True
    else:
        other_args = "bar", (True,)

    # revealed: tuple[Literal["foo"], Literal[True]] | tuple[Literal["bar"], tuple[Literal[True]]]
    reveal_type(other_args)
    # error: [invalid-argument-type] "Argument to function `f2` is incorrect: Expected `bool`, found `Literal[True] | tuple[Literal[True]]`"
    f2(*other_args)

def f4(a=None, b=None, c=None, d=None, e=None): ...

my_args = ((1, 2), (3, 4), (5, 6))

for tup in my_args:
    f4(*tup, e=None)  # fine

my_other_args = (
    (1, 2, 3, 4, 5),
    (6, 7, 8, 9, 10),
)

for tup in my_other_args:
    # error: [parameter-already-assigned] "Multiple values provided for parameter `e` of function `f4`"
    f4(*tup, e=None)
```

### Mixed argument and parameter containing variadic

```toml
[environment]
python-version = "3.11"
```

```py
def f(x: int, *args: str) -> int:
    return 1

def _(
    args1: list[int],
    args2: tuple[int],
    args3: tuple[int, int],
    args4: tuple[int, ...],
    args5: tuple[int, *tuple[str, ...]],
    args6: tuple[int, int, *tuple[str, ...]],
) -> None:
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `int`"
    reveal_type(f(*args1))  # revealed: int

    # This shouldn't raise an error because the unpacking doesn't match the variadic parameter.
    reveal_type(f(*args2))  # revealed: int

    # But, this should because the second tuple element is not assignable.
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `int`"
    reveal_type(f(*args3))  # revealed: int

    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `int`"
    reveal_type(f(*args4))  # revealed: int

    # The first element of the tuple matches the required argument;
    # all subsequent elements match the variadic argument
    reveal_type(f(*args5))  # revealed: int

    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `int`"
    reveal_type(f(*args6))  # revealed: int
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

### Diagnostics for union types where the union is not assignable

<!-- snapshot-diagnostics -->

```py
from typing import Sized

class Foo: ...
class Bar: ...
class Baz: ...

def f(x: Sized): ...
def g(
    a: str | Foo,
    b: list[str] | str | dict[str, str] | tuple[str, ...] | bytes | frozenset[str] | set[str] | Foo,
    c: list[str] | str | dict[str, str] | tuple[str, ...] | bytes | frozenset[str] | set[str] | Foo | Bar,
    d: list[str] | str | dict[str, str] | tuple[str, ...] | bytes | frozenset[str] | set[str] | Foo | Bar | Baz,
):
    f(a)  # error: [invalid-argument-type]
    f(b)  # error: [invalid-argument-type]
    f(c)  # error: [invalid-argument-type]
    f(d)  # error: [invalid-argument-type]
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

## Keywords argument

A double-starred argument (`**kwargs`) can be used to pass an argument that implements the mapping
protocol. This is matched against any of the *unmatched* standard (positional or keyword),
keyword-only, and keywords (`**kwargs`) parameters.

### Empty

```py
def empty() -> None: ...
def _(kwargs: dict[str, int]) -> None:
    empty(**kwargs)

empty(**{})
empty(**dict())
```

### Single parameter

```py
from typing_extensions import TypedDict

def f(**kwargs: int) -> None: ...

class Foo(TypedDict):
    a: int
    b: int

def _(kwargs: dict[str, int]) -> None:
    f(**kwargs)

f(**{"foo": 1})
f(**dict(foo=1))
f(**Foo(a=1, b=2))
```

### Positional-only and variadic parameters

```py
def f1(a: int, b: int, /) -> None: ...
def f2(*args: int) -> None: ...
def _(kwargs: dict[str, int]) -> None:
    # error: [missing-argument] "No arguments provided for required parameters `a`, `b` of function `f1`"
    f1(**kwargs)

    # This doesn't raise an error because `*args` is an optional parameter and `**kwargs` can be empty.
    f2(**kwargs)
```

### Standard parameters

```py
from typing_extensions import TypedDict

class Foo(TypedDict):
    a: int
    b: int

def f(a: int, b: int) -> None: ...
def _(kwargs: dict[str, int]) -> None:
    f(**kwargs)

f(**{"a": 1, "b": 2})
f(**dict(a=1, b=2))
f(**Foo(a=1, b=2))
```

### Keyword-only parameters

```py
from typing_extensions import TypedDict

class Foo(TypedDict):
    a: int
    b: int

def f(*, a: int, b: int) -> None: ...
def _(kwargs: dict[str, int]) -> None:
    f(**kwargs)

f(**{"a": 1, "b": 2})
f(**dict(a=1, b=2))
f(**Foo(a=1, b=2))
```

### Multiple keywords argument

```py
def f(**kwargs: int) -> None: ...
def _(kwargs1: dict[str, int], kwargs2: dict[str, int], kwargs3: dict[str, str], kwargs4: dict[int, list]) -> None:
    f(**kwargs1, **kwargs2)
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `str`"
    f(**kwargs1, **kwargs3)
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `str`"
    # error: [invalid-argument-type] "Argument expression after ** must be a mapping with `str` key type: Found `int`"
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `list[Unknown]`"
    f(**kwargs3, **kwargs4)
```

### Keyword-only after keywords

```py
class B: ...

def f(*, a: int, b: B, **kwargs: int) -> None: ...
def _(kwargs: dict[str, int]):
    # Make sure that the `b` argument is not being matched against `kwargs` by passing an integer
    # instead of the annotated type which should raise an
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `B`, found `Literal[2]`"
    f(a=1, **kwargs, b=2)
```

### Mixed parameter kind

```py
def f1(*, a: int, b: int, **kwargs: int) -> None: ...
def f2(a: int, *, b: int, **kwargs: int) -> None: ...
def f3(a: int, /, *args: int, b: int, **kwargs: int) -> None: ...
def _(kwargs1: dict[str, int], kwargs2: dict[str, str]):
    f1(**kwargs1)
    f2(**kwargs1)
    f3(1, **kwargs1)
```

### TypedDict

```py
from typing_extensions import NotRequired, TypedDict

class Foo1(TypedDict):
    a: int
    b: str

class Foo2(TypedDict):
    a: int
    b: NotRequired[str]

def f(**kwargs: int) -> None: ...

# error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `str`"
f(**Foo1(a=1, b="b"))
# error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `int`, found `str`"
f(**Foo2(a=1))
```

### Keys must be strings

The keys of the mapping passed to a double-starred argument must be strings.

```py
from collections.abc import Mapping

def f(**kwargs: int) -> None: ...

class DictSubclass(dict[int, int]): ...
class MappingSubclass(Mapping[int, int]): ...

class MappingProtocol:
    def keys(self) -> list[int]:
        return [1]

    def __getitem__(self, key: int) -> int:
        return 1

def _(kwargs: dict[int, int]) -> None:
    # error: [invalid-argument-type] "Argument expression after ** must be a mapping with `str` key type: Found `int`"
    f(**kwargs)

# error: [invalid-argument-type] "Argument expression after ** must be a mapping with `str` key type: Found `int`"
f(**DictSubclass())
# error: [invalid-argument-type] "Argument expression after ** must be a mapping with `str` key type: Found `int`"
f(**MappingSubclass())
# error: [invalid-argument-type] "Argument expression after ** must be a mapping with `str` key type: Found `int`"
f(**MappingProtocol())
```

The key can also be a custom type that inherits from `str`.

```py
class SubStr(str): ...
class SubInt(int): ...

def _(kwargs1: dict[SubStr, int], kwargs2: dict[SubInt, int]) -> None:
    f(**kwargs1)
    # error: [invalid-argument-type] "Argument expression after ** must be a mapping with `str` key type: Found `SubInt`"
    f(**kwargs2)
```

Or, it can be a type that is assignable to `str`.

```py
from typing import Any
from ty_extensions import Unknown

def _(kwargs1: dict[Any, int], kwargs2: dict[Unknown, int]) -> None:
    f(**kwargs1)
    f(**kwargs2)
```

### Invalid value type

```py
from collections.abc import Mapping

def f(**kwargs: str) -> None: ...

class DictSubclass(dict[str, int]): ...
class MappingSubclass(Mapping[str, int]): ...

class MappingProtocol:
    def keys(self) -> list[str]:
        return ["foo"]

    def __getitem__(self, key: str) -> int:
        return 1

def _(kwargs: dict[str, int]) -> None:
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `int`"
    f(**kwargs)
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `int`"
    f(**DictSubclass())
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `int`"
    f(**MappingSubclass())
    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `str`, found `int`"
    f(**MappingProtocol())
```

### `Unknown` type

```py
from ty_extensions import Unknown

def f(**kwargs: int) -> None: ...
def _(kwargs: Unknown):
    f(**kwargs)
```

### Not a mapping

```py
def f(**kwargs: int) -> None: ...

class A: ...

class InvalidMapping:
    def keys(self) -> A:
        return A()

    def __getitem__(self, key: str) -> int:
        return 1

def _(kwargs: dict[str, int] | int):
    # error: [invalid-argument-type] "Argument expression after ** must be a mapping type: Found `dict[str, int] | int`"
    f(**kwargs)
    # error: [invalid-argument-type] "Argument expression after ** must be a mapping type: Found `InvalidMapping`"
    f(**InvalidMapping())
```

### Generic

For a generic keywords parameter, the type variable should be specialized to the value type of the
mapping.

```py
from typing import TypeVar

_T = TypeVar("_T")

def f(**kwargs: _T) -> _T:
    return kwargs["a"]

def _(kwargs: dict[str, int]) -> None:
    reveal_type(f(**kwargs))  # revealed: int
```

For a `TypedDict`, the type variable should be specialized to the union of all value types.

```py
from typing import TypeVar
from typing_extensions import TypedDict

_T = TypeVar("_T")

class Foo(TypedDict):
    a: int
    b: str

def f(**kwargs: _T) -> _T:
    return kwargs["a"]

reveal_type(f(**Foo(a=1, b="b")))  # revealed: int | str
```
