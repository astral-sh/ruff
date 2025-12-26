# Calling a union of function types

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.12"
```

## A smaller scale example

```py
def f1() -> int:
    return 0

def f2(name: str) -> int:
    return 0

def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2
    # error: [too-many-positional-arguments]
    # error: [invalid-argument-type]
    x = f(3)
```

## Multiple variants but only one is invalid

This test in particular demonstrates some of the smarts of this diagnostic. Namely, since only one
variant is invalid, additional context specific to that variant is added to the diagnostic output.
(If more than one variant is invalid, then this additional context is elided to avoid overwhelming
the end user.)

```py
def f1(a: int) -> int:
    return 0

def f2(name: str) -> int:
    return 0

def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2
    # error: [invalid-argument-type]
    x = f(3)
```

## Try to cover all possible reasons

These tests is likely to become stale over time, but this was added when the union-specific
diagnostic was initially created. In each test, we try to cover as much as we can. This is mostly
just ensuring that we get test coverage for each of the possible diagnostic messages.

### Cover non-keyword related reasons

```py
from inspect import getattr_static
from typing import overload

def f1() -> int:
    return 0

def f2(name: str) -> int:
    return 0

def f3(a: int, b: int) -> int:
    return 0

def f4[T: str](x: T) -> int:
    return 0

@overload
def f5() -> None: ...
@overload
def f5(x: str) -> str: ...
def f5(x: str | None = None) -> str | None:
    return x

@overload
def f6() -> None: ...
@overload
def f6(x: str, y: str) -> str: ...
def f6(x: str | None = None, y: str | None = None) -> str | None:
    return x + y if x and y else None

def _(n: int):
    class PossiblyNotCallable:
        if n == 0:
            def __call__(self) -> int:
                return 0

    if n == 0:
        f = f1
    elif n == 1:
        f = f2
    elif n == 2:
        f = f3
    elif n == 3:
        f = f4
    elif n == 4:
        f = 5
    elif n == 5:
        f = f5
    elif n == 6:
        f = f6
    else:
        f = PossiblyNotCallable()
    # error: [too-many-positional-arguments]
    # error: [invalid-argument-type] "Argument to function `f2` is incorrect: Expected `str`, found `Literal[3]`"
    # error: [missing-argument]
    # error: [invalid-argument-type] "Argument to function `f4` is incorrect: Argument type `Literal[3]` does not satisfy upper bound `str` of type variable `T`"
    # error: [invalid-argument-type] "Argument to function `f5` is incorrect: Expected `str`, found `Literal[3]`"
    # error: [no-matching-overload] "No overload of function `f6` matches arguments"
    # error: [call-non-callable] "Object of type `Literal[5]` is not callable"
    # error: [call-non-callable] "Object of type `PossiblyNotCallable` is not callable (possibly missing `__call__` method)"
    x = f(3)
```

### Cover keyword argument related reasons

```py
def any(*args, **kwargs) -> int:
    return 0

def f1(name: str) -> int:
    return 0

def _(n: int):
    if n == 0:
        f = f1
    else:
        f = any
    # error: [parameter-already-assigned]
    # error: [unknown-argument]
    y = f("foo", name="bar", unknown="quux")
```

### Truncation for long unions and literals

This test demonstrates a call where the expected type is a large mixed union. The diagnostic must
therefore truncate the long expected union type to avoid overwhelming output.

```py
from typing import Literal, Union

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...

def f1(x: Union[Literal[1, 2, 3, 4, 5, 6, 7, 8], A, B, C, D, E, F]) -> int:
    return 0

def _(n: int):
    x = n
    # error: [invalid-argument-type]
    f1(x)
```
