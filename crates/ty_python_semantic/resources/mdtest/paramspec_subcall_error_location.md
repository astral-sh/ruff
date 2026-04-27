# `ParamSpec` error locations

When a free `ParamSpec` is available in a parameter before the ones representing it's components
(`P.args` and `P.kwargs`), ty invokes a sub-call logic where it performs a separate call to the
function with the arguments that are resolved from the `ParamSpec`. In this case, the diagnostic
location need to be offset based on the position of the `ParamSpec` components.

```toml
[environment]
python-version = "3.12"
```

## Functions

```py
from typing import Callable

def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
def fn1(a: int, b: int, c: int) -> None: ...

# snapshot: invalid-argument-type
# snapshot: invalid-argument-type
# snapshot: unknown-argument
foo(fn1, "a", 2, c="c", unknown=1)
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:9:10
  |
9 | foo(fn1, "a", 2, c="c", unknown=1)
  |          ^^^ Expected `int`, found `Literal["a"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:3:5
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |     ^^^         ------------------ Parameter declared here
  |


error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:9:18
  |
9 | foo(fn1, "a", 2, c="c", unknown=1)
  |                  ^^^^^ Expected `int`, found `Literal["c"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:3:5
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |     ^^^                                            ------------------ Parameter declared here
  |


error[unknown-argument]: Argument `unknown` does not match any known parameter of function `foo`
 --> src/mdtest_snippet.py:9:25
  |
9 | foo(fn1, "a", 2, c="c", unknown=1)
  |                         ^^^^^^^^^
  |
info: Function signature here
 --> src/mdtest_snippet.py:3:5
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
```

```py
def fn2(a: int) -> None: ...

# snapshot: too-many-positional-arguments
foo(fn2, 1, 2, 3)
```

```snapshot
error[too-many-positional-arguments]: Too many positional arguments to function `foo`: expected 1, got 3
  --> src/mdtest_snippet.py:13:13
   |
13 | foo(fn2, 1, 2, 3)
   |             ^
   |
info: Function signature here
 --> src/mdtest_snippet.py:3:5
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
```

```py
def fn3(a: int, /) -> None: ...

# snapshot: positional-only-parameter-as-kwarg
foo(fn3, a=1)
```

```snapshot
error[positional-only-parameter-as-kwarg]: Positional-only parameter 1 (`a`) passed as keyword argument of function `foo`
  --> src/mdtest_snippet.py:17:10
   |
17 | foo(fn3, a=1)
   |          ^^^
   |
info: Function signature here
 --> src/mdtest_snippet.py:3:5
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
```

```py
def fn4(a: int, b: int) -> None: ...

# snapshot: parameter-already-assigned
# snapshot: missing-argument
foo(fn4, 1, a=2)

# snapshot: missing-argument
foo(fn4)
```

```snapshot
error[missing-argument]: No argument provided for required parameter `b` of function `foo`
  --> src/mdtest_snippet.py:22:1
   |
22 | foo(fn4, 1, a=2)
   | ^^^^^^^^^^^^^^^^
   |
info: Parameter declared here
 --> src/mdtest_snippet.py:3:37
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |                                     ^^^^^^^^^^^^^
  |


error[parameter-already-assigned]: Multiple values provided for parameter `a` of function `foo`
  --> src/mdtest_snippet.py:22:13
   |
22 | foo(fn4, 1, a=2)
   |             ^^^
   |


error[missing-argument]: No arguments provided for required parameters `a`, `b` of function `foo`
  --> src/mdtest_snippet.py:25:1
   |
25 | foo(fn4)
   | ^^^^^^^^
   |
info: Parameters declared here
 --> src/mdtest_snippet.py:3:16
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
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
