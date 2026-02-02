# test for paramspec error location

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
def bar[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...

# error: [invalid-argument-type]
# error: [invalid-argument-type]
# error: [invalid-argument-type]
bar(fn2, a="a", b=2, c="c")
```

Unknown Argument

```py
from typing import Callable

def fn(a: int) -> None: ...
def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...

foo(fn, a=1, unknown_param=2)  # error: [unknown-argument]
```

Position Only Parameter As Kwarg

```py
from typing import Callable

def fn(a: int, /) -> None: ...
def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...

foo(fn, a=1)  # error: [positional-only-parameter-as-kwarg]
```

Parameter Already Assigned

```py
from typing import Callable

def fn(a: int, b: int) -> None: ...
def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...

# error: [parameter-already-assigned]
# error: [missing-argument]
foo(fn, 1, a=2)
```

Too Many Positional Arguments

```py
from typing import Callable

def fn(a: int) -> None: ...
def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...

foo(fn, 1, 2, 3)  # error: [too-many-positional-arguments]
```
