# `ParamSpec` error locations

When a free `ParamSpec` is available in a parameter before the ones representing it's components
(`P.args` and `P.kwargs`), ty invokes a sub-call logic where it performs a separate call to the
function with the arguments that are resolved from the `ParamSpec`. In this case, the diagnostic
location need to be offset based on the position of the `ParamSpec` components.

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.12"
```

## Functions

```py
from typing import Callable

def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
def fn1(a: int, b: int, c: int) -> None: ...

# error: [invalid-argument-type]
# error: [invalid-argument-type]
# error: [unknown-argument]
foo(fn1, "a", 2, c="c", unknown=1)

def fn2(a: int) -> None: ...

# error: [too-many-positional-arguments]
foo(fn2, 1, 2, 3)

def fn3(a: int, /) -> None: ...

# error: [positional-only-parameter-as-kwarg]
foo(fn3, a=1)

def fn4(a: int, b: int) -> None: ...

# error: [parameter-already-assigned]
# error: [missing-argument]
foo(fn4, 1, a=2)

# error: [missing-argument]
foo(fn4)
```

## Methods

Methods require additional logic to offset the location given the additional synthetic `self`
parameter.

```py
from typing import Callable

class Foo:
    def method[**P, T](self, fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...

def fn1(a: int, b: int, c: int) -> None: ...

foo = Foo()

# error: [invalid-argument-type]
# error: [invalid-argument-type]
# error: [unknown-argument]
foo.method(fn1, "a", 2, c="c", unknown=1)
```
