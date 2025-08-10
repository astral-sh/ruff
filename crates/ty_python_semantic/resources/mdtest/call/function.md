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
    # TODO: we should emit `[too-many-positional-arguments]` here
    takes_zero(*args)

    takes_one(*args)

    # TODO: we should emit `[missing-argument]` on both of these
    takes_two(*args)
    takes_two_positional_only(*args)

    # TODO: these should both be `[missing-argument]`, not `[invalid-argument-type]`
    takes_two_different(*args)  # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)  # error: [invalid-argument-type]

    takes_at_least_zero(*args)
    takes_at_least_one(*args)

    # TODO: we should emit `[missing-argument]` on both of these
    takes_at_least_two(*args)
    takes_at_least_two_positional_only(*args)

class TwoElementIntTuple(tuple[int, int]): ...

def _(args: TwoElementIntTuple) -> None:
    # TODO: we should emit `[too-many-positional-arguments]` on both of these
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

class IntStrTuple(tuple[int, str]): ...

def _(args: IntStrTuple) -> None:
    # TODO: we should emit `[too-many-positional-arguments]` here
    takes_zero(*args)

    # TODO: this should be `[too-many-positional-arguments]`, not `[invalid-argument-type]`
    takes_one(*args)  # error: [invalid-argument-type]

    # TODO: we should have one diagnostic for each of these, not two
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_positional_only(*args)

    # TODO: these are all false positives
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_different(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)

    takes_at_least_zero(*args)

    # TODO: false positive
    # error: [invalid-argument-type]
    takes_at_least_one(*args)

    # TODO: we should only emit one diagnostic for each of these, not two
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_at_least_two(*args)
    # error: [invalid-argument-type]
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
    # TODO: we should emit `[too-many-positional-arguments]` here
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

class IntStarStr(tuple[int, *tuple[str, ...]]): ...

def _(args: IntStarStr) -> None:
    # TODO: we should emit `[too-many-positional-arguments]` here
    takes_zero(*args)

    # TODO: false positive
    # error: [invalid-argument-type]
    takes_one(*args)

    # TODO: we should only emit one diagnostic for each of these, not two
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_positional_only(*args)

    # TODO: false positives
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_different(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)

    takes_at_least_zero(*args)

    # TODO: false positive
    # error: [invalid-argument-type]
    takes_at_least_one(*args)

    # TODO: we should only have one diagnostic for each of these, not two
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_at_least_two(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_at_least_two_positional_only(*args)

class IntIntStarInt(tuple[int, int, *tuple[int, ...]]): ...

def _(args: IntIntStarInt) -> None:
    # TODO: we should emit `[too-many-positional-arguments]` on both of these
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

class IntIntStarStr(tuple[int, int, *tuple[str, ...]]): ...

def _(args: IntIntStarStr) -> None:
    # TODO: we should emit `[too-many-positional-arguments]` here
    takes_zero(*args)

    # TODO: this should be `[too-many-positional-arguments]`, not `invalid-argument-type`
    takes_one(*args)  # error: [invalid-argument-type]

    # TODO: these are all false positives
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_positional_only(*args)

    # TODO: each of these should only have one diagnostic, not two
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_different(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)

    takes_at_least_zero(*args)

    # TODO: false positive
    # error: [invalid-argument-type]
    takes_at_least_one(*args)

    # TODO: these are both false positives
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_at_least_two(*args)

    # TODO: these are both false positives
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_at_least_two_positional_only(*args)

class IntStarIntInt(tuple[int, *tuple[int, ...], int]): ...

def _(args: IntStarIntInt) -> None:
    # TODO: we should emit `[too-many-positional-arguments]` on both of these
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

class IntStarStrInt(tuple[int, *tuple[str, ...], int]): ...

def _(args: IntStarStrInt) -> None:
    # TODO: we should emit `too-many-positional-arguments` here
    takes_zero(*args)

    # TODO: this should be `too-many-positional-arguments`, not `invalid-argument-type`
    takes_one(*args)  # error: [invalid-argument-type]

    # TODO: we should only emit one diagnostic for each of these
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_positional_only(*args)

    # TODO: we should not emit diagnostics for these
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_different(*args)
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_two_different_positional_only(*args)

    takes_at_least_zero(*args)

    # TODO: false positive
    takes_at_least_one(*args)  # error: [invalid-argument-type]

    # TODO: should only have one diagnostic here
    # error: [invalid-argument-type]
    # error: [invalid-argument-type]
    takes_at_least_two(*args)

    # TODO: should only have one diagnostic here
    # error: [invalid-argument-type]
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

### Argument expansion regression

This is a regression that was highlighted by the ecosystem check, which shows that we might need to
rethink how we perform argument expansion during overload resolution. In particular, we might need
to retry both `match_parameters` _and_ `check_types` for each expansion. Currently we only retry
`check_types`.

The issue is that argument expansion might produce a splatted value with a different arity than what
we originally inferred for the unexpanded value, and that in turn can affect which parameters the
splatted value is matched with.

The first example correctly produces an error. The `tuple[int, str]` union element has a precise
arity of two, and so parameter matching chooses the first overload. The second element of the tuple
does not match the second parameter type, which yielding an `invalid-argument-type` error.

The third example should produce the same error. However, because we have a union, we do not see the
precise arity of each union element during parameter matching. Instead, we infer an arity of "zero
or more" for the union as a whole, and use that less precise arity when matching parameters. We
therefore consider the second overload to still be a potential candidate for the `tuple[int, str]`
union element. During type checking, we have to force the arity of each union element to match the
inferred arity of the union as a whole (turning `tuple[int, str]` into `tuple[int | str, ...]`).
That less precise tuple type-checks successfully against the second overload, making us incorrectly
think that `tuple[int, str]` is a valid splatted call.

If we update argument expansion to retry parameter matching with the precise arity of each union
element, we will correctly rule out the second overload for `tuple[int, str]`, just like we do when
splatting that tuple directly (instead of as part of a union).

```py
from typing import overload

@overload
def f(x: int, y: int) -> None: ...
@overload
def f(x: int, y: str, z: int) -> None: ...
def f(*args): ...

# Test all of the above with a number of different splatted argument types

def _(t: tuple[int, str]) -> None:
    f(*t)  # error: [invalid-argument-type]

def _(t: tuple[int, str, int]) -> None:
    f(*t)

def _(t: tuple[int, str] | tuple[int, str, int]) -> None:
    # TODO: error: [invalid-argument-type]
    f(*t)
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
