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

def f1() -> int:
    return 0

def f2(name: str) -> int:
    return 0

def f3(a: int, b: int) -> int:
    return 0

def f4[T: str](x: T) -> int:
    return 0

class OverloadExample:
    def f(self, x: str) -> int:
        return 0

f5 = getattr_static(OverloadExample, "f").__get__

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
    else:
        f = PossiblyNotCallable()
    # error: [too-many-positional-arguments]
    # error: [invalid-argument-type] "Argument to function `f2` is incorrect: Expected `str`, found `Literal[3]`"
    # error: [missing-argument]
    # error: [invalid-argument-type] "Argument to function `f4` is incorrect: Argument type `Literal[3]` does not satisfy upper bound of type variable `T`"
    # error: [call-non-callable] "Object of type `Literal[5]` is not callable"
    # error: [no-matching-overload]
    # error: [call-non-callable] "Object of type `PossiblyNotCallable` is not callable (possibly unbound `__call__` method)"
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
