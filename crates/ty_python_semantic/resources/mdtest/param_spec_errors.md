# `ParamSpec` error locations

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.12"
```

Invalid Argument

```py
from typing import Callable

def fn1(a: int) -> None: ...
def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...

foo(fn1, a="a")  # error: [invalid-argument-type]

def fn2(a: int, b: str, c: float) -> None: ...

# error: [invalid-argument-type]
# error: [invalid-argument-type]
# error: [invalid-argument-type]
foo(fn2, a="a", b=2, c="c")

def fn3(a: int) -> None: ...

foo(fn3, a=1, unknown_param=2)  # error: [unknown-argument]
foo(fn3, 1, 2, 3)  # error: [too-many-positional-arguments]

def fn4(a: int, /) -> None: ...

foo(fn4, a=1)  # error: [positional-only-parameter-as-kwarg]

def fn5(a: int, b: int) -> None: ...

# error: [parameter-already-assigned]
# error: [missing-argument]
foo(fn5, 1, a=2)
```
