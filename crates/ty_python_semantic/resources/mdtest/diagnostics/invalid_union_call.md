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
    x = f(3)  # error: [invalid-union-call]
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
    x = f(3)  # error: [invalid-union-call]
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
    x = f(3)  # error: [invalid-union-call]
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
    y = f("foo", name="bar", unknown="quux")  # error: [invalid-union-call]
```
