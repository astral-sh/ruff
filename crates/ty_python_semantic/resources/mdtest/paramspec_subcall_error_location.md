# `ParamSpec` error locations

A callable can accept another callable and forward its positional and keyword arguments using a
`ParamSpec`. These tests check that argument errors identify the callback parameter that rejected
the argument, or fall back to the forwarding function's `*args` or `**kwargs` when that parameter
cannot be identified.

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
info: Function defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def fn1(a: int, b: int, c: int) -> None: ...
  |     ^^^ ------ Parameter declared here


error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:9:18
  |
9 | foo(fn1, "a", 2, c="c", unknown=1)
  |                  ^^^^^ Expected `int`, found `Literal["c"]`
info: Function defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def fn1(a: int, b: int, c: int) -> None: ...
  |     ^^^                 ------ Parameter declared here


error[unknown-argument]: Argument `unknown` does not match any known parameter of function `foo`
 --> src/mdtest_snippet.py:9:25
  |
9 | foo(fn1, "a", 2, c="c", unknown=1)
  |                         ^^^^^^^^^
info: Function signature here
 --> src/mdtest_snippet.py:3:5
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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
info: Function signature here
 --> src/mdtest_snippet.py:3:5
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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
info: Function signature here
 --> src/mdtest_snippet.py:3:5
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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
info: Parameter declared here
 --> src/mdtest_snippet.py:3:37
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |                                     ^^^^^^^^^^^^^


error[parameter-already-assigned]: Multiple values provided for parameter `a` of function `foo`
  --> src/mdtest_snippet.py:22:13
   |
22 | foo(fn4, 1, a=2)
   |             ^^^


error[missing-argument]: No arguments provided for required parameters `a`, `b` of function `foo`
  --> src/mdtest_snippet.py:25:1
   |
25 | foo(fn4)
   | ^^^^^^^^
info: Parameters declared here
 --> src/mdtest_snippet.py:3:16
  |
3 | def foo[**P, T](fn: Callable[P, T], *args: P.args, **kwargs: P.kwargs): ...
  |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
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

## Forwarded keyword arguments

A forwarded keyword argument should identify the matching callback parameter, not the parameter that
accepts the callback.

```py
from typing import Callable

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
def callback(*, value: int) -> None: ...

wrapper(callback, value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:6:19
  |
6 | wrapper(callback, value="incorrect")  # snapshot: invalid-argument-type
  |                   ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def callback(*, value: int) -> None: ...
  |     ^^^^^^^^    ---------- Parameter declared here
```

## Forwarded bound methods

A bound method still includes `self` in its source signature. The diagnostic should skip that
parameter and identify the argument that actually failed.

```py
from typing import Callable

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Handler:
    def callback(self, *, value: int) -> None: ...

def run(handler: Handler) -> None:
    wrapper(handler.callback, value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:9:31
  |
9 |     wrapper(handler.callback, value="incorrect")  # snapshot: invalid-argument-type
  |                               ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Method defined here
 --> src/mdtest_snippet.py:6:9
  |
6 |     def callback(self, *, value: int) -> None: ...
  |         ^^^^^^^^          ---------- Parameter declared here
```

## Callbacks without a source definition

A `Callable` annotation describes the accepted arguments but does not identify the function that
declared them. In that case, point to the forwarding function's `*args` parameter. The expanded
keyword parameters below cover the corresponding `**kwargs` fallback.

```py
from typing import Callable

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
def run(callback: Callable[[int], None]) -> None:
    wrapper(callback, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:5:23
  |
5 |     wrapper(callback, "incorrect")  # snapshot: invalid-argument-type
  |                       ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:3:5
  |
3 | def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
  |     ^^^^^^^                                   ------------- Parameter declared here
```

## Parameters consumed by Concatenate

`Concatenate` lets a forwarding function provide the first argument itself. The diagnostic still
needs to account for that argument when locating the callback's remaining parameter.

```py
from typing import Callable, Concatenate

def wrapper[**P](callback: Callable[Concatenate[int, P], None], *args: P.args, **kwargs: P.kwargs) -> None:
    callback(0, *args, **kwargs)

def callback(prefix: int, *, value: int) -> None: ...

wrapper(callback, value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:8:19
  |
8 | wrapper(callback, value="incorrect")  # snapshot: invalid-argument-type
  |                   ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:6:5
  |
6 | def callback(prefix: int, *, value: int) -> None: ...
  |     ^^^^^^^^                 ---------- Parameter declared here
```

## Overloaded callbacks

When a callback has multiple overloads, the diagnostic should identify the parameter on the overload
that accepted the other arguments.

```py
from typing import Callable, overload

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
@overload
def callback(value: int) -> None: ...
@overload
def callback(value: str, *, flag: str) -> None: ...
def callback(value: int | str, *, flag: str | None = None) -> None: ...

wrapper(callback, "value", flag=1)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:10:28
   |
10 | wrapper(callback, "value", flag=1)  # snapshot: invalid-argument-type
   |                            ^^^^^^ Expected `str`, found `Literal[1]`
info: Function defined here
 --> src/mdtest_snippet.py:7:5
  |
7 | def callback(value: str, *, flag: str) -> None: ...
  |     ^^^^^^^^                --------- Parameter declared here
```

## Overloads selected by Concatenate

The first callback overload accepts a `str` prefix, so it cannot match a forwarding function that
always supplies an `int`. An error in the remaining arguments should point to the second overload.

```py
from typing import Callable, Concatenate, overload

def wrapper[**P](callback: Callable[Concatenate[int, P], None], *args: P.args, **kwargs: P.kwargs) -> None:
    callback(1, *args, **kwargs)

@overload
def callback(prefix: str, value: str) -> None: ...
@overload
def callback(prefix: int, value: int) -> None: ...
def callback(prefix: str | int, value: str | int) -> None: ...

wrapper(callback, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:12:19
   |
12 | wrapper(callback, "incorrect")  # snapshot: invalid-argument-type
   |                   ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:9:5
  |
9 | def callback(prefix: int, value: int) -> None: ...
  |     ^^^^^^^^              ---------- Parameter declared here
```

## Overloads selected by a bound receiver

A generic method can have separate overloads for different receiver types. A method on
`Receiver[int]` should point to the overload declared for `Receiver[int]`.

```py
from typing import Callable, overload

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Receiver[T]:
    value: T

    @overload
    def method(self: "Receiver[str]", value: str) -> None: ...
    @overload
    def method(self: "Receiver[int]", value: int) -> None: ...
    def method(self, value: str | int) -> None: ...

def run(receiver: Receiver[int]) -> None:
    wrapper(receiver.method, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:15:30
   |
15 |     wrapper(receiver.method, "incorrect")  # snapshot: invalid-argument-type
   |                              ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Method defined here
  --> src/mdtest_snippet.py:11:9
   |
11 |     def method(self: "Receiver[int]", value: int) -> None: ...
   |         ^^^^^^                        ---------- Parameter declared here
```

## Callback annotations with multiple callable alternatives

The callback below matches the second union alternative, which does not consume a leading argument.
The diagnostic should therefore identify `first`, not `second`.

```py
from typing import Callable, Concatenate

def wrapper[**P](callback: Callable[Concatenate[int, P], None] | Callable[P, str], *args: P.args, **kwargs: P.kwargs) -> None: ...
def callback(first: int, second: str) -> str:
    return second

wrapper(callback, "incorrect", "valid")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:7:19
  |
7 | wrapper(callback, "incorrect", "valid")  # snapshot: invalid-argument-type
  |                   ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def callback(first: int, second: str) -> str:
  |     ^^^^^^^^ ---------- Parameter declared here
```

## Optional callbacks

A union may also contain a value that is not callable. The presence of `None` should not prevent the
diagnostic from identifying the callback's parameter.

```py
from typing import Callable

def wrapper[**P](callback: Callable[P, None] | None, *args: P.args, **kwargs: P.kwargs) -> None: ...
def callback(value: int) -> None: ...

wrapper(callback, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:6:19
  |
6 | wrapper(callback, "incorrect")  # snapshot: invalid-argument-type
  |                   ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def callback(value: int) -> None: ...
  |     ^^^^^^^^ ---------- Parameter declared here
```

## Overloaded forwarding functions

An overloaded forwarding function should retain the note identifying its matching overload as well
as the note identifying the callback parameter.

```py
from typing import Callable, overload

@overload
def wrap[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
@overload
def wrap(value: int, first: int, second: int) -> None: ...
def wrap(callback: Callable[..., None] | int, *args: object, **kwargs: object) -> None: ...
def keyword_callback(*, value: int) -> None: ...
def positional_callback(*values: int) -> None: ...
```

A keyword argument belongs to the forwarding function's `**kwargs` parameter.

```py
wrap(keyword_callback, value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrap` is incorrect
  --> src/mdtest_snippet.py:10:24
   |
10 | wrap(keyword_callback, value="incorrect")  # snapshot: invalid-argument-type
   |                        ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:8:5
  |
8 | def keyword_callback(*, value: int) -> None: ...
  |     ^^^^^^^^^^^^^^^^    ---------- Parameter declared here
info: Matching overload defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def wrap[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
  |     ^^^^                                                  ------------------ Parameter declared here
info: Non-matching overloads for function `wrap`:
info:   (value: int, first: int, second: int) -> None
```

A positional argument belongs to `*args`, even when the callback accepts it through `*values`.

```py
wrap(positional_callback, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrap` is incorrect
  --> src/mdtest_snippet.py:11:27
   |
11 | wrap(positional_callback, "incorrect")  # snapshot: invalid-argument-type
   |                           ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:9:5
  |
9 | def positional_callback(*values: int) -> None: ...
  |     ^^^^^^^^^^^^^^^^^^^ ------------ Parameter declared here
info: Matching overload defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def wrap[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
  |     ^^^^                                   ------------- Parameter declared here
info: Non-matching overloads for function `wrap`:
info:   (value: int, first: int, second: int) -> None
```

## Forwarding through a callable object

The forwarding object's own `self` parameter is not the callback. The diagnostic should point to the
argument accepted by `callback`.

```py
from typing import Callable

class Wrapper:
    def __call__[**P](self, callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...

def callback(value: int) -> None: ...

Wrapper()(callback, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to bound method `Wrapper.__call__` is incorrect
 --> src/mdtest_snippet.py:8:21
  |
8 | Wrapper()(callback, "incorrect")  # snapshot: invalid-argument-type
  |                     ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:6:5
  |
6 | def callback(value: int) -> None: ...
  |     ^^^^^^^^ ---------- Parameter declared here
```

## Overloads with expanded positional parameters

Expanding `Unpack[tuple[int]]` must preserve the link to the callback's `*values` declaration. The
diagnostic can then identify the matching overload instead of falling back to the forwarding
function's `*args` parameter.

```py
from typing import Callable, Concatenate, Unpack, overload

def wrapper[**P](callback: Callable[Concatenate[int, P], None], *args: P.args, **kwargs: P.kwargs) -> None: ...
@overload
def callback(prefix: str, *values: Unpack[tuple[str]]) -> None: ...
@overload
def callback(prefix: int, *values: Unpack[tuple[int]]) -> None: ...
def callback(prefix: str | int, *values: Unpack[tuple[str | int]]) -> None: ...

wrapper(callback, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:10:19
   |
10 | wrapper(callback, "incorrect")  # snapshot: invalid-argument-type
   |                   ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:7:5
  |
7 | def callback(prefix: int, *values: Unpack[tuple[int]]) -> None: ...
  |     ^^^^^^^^              --------------------------- Parameter declared here
```

## Expanded keyword parameters

`Unpack[Config]` creates separate keyword parameters for `alpha` and `beta`, even though the
callback declares only `**options`. An error for `beta` should point to that `**options`
declaration.

```py
from typing import Callable, TypedDict, Unpack

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Config(TypedDict):
    alpha: int
    beta: int

def callback(**options: Unpack[Config]) -> None: ...

wrapper(callback, alpha=1, beta="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:11:28
   |
11 | wrapper(callback, alpha=1, beta="incorrect")  # snapshot: invalid-argument-type
   |                            ^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:9:5
  |
9 | def callback(**options: Unpack[Config]) -> None: ...
  |     ^^^^^^^^ ------------------------- Parameter declared here
```

## Overloads with expanded keyword parameters

Both callback overloads unpack the same `TypedDict`, so their expanded parameters refer to the same
field declarations. The diagnostic should still identify the overload selected by its `int` prefix.

```py
from typing import Callable, Concatenate, TypedDict, Unpack, overload

class Config(TypedDict):
    value: int

def wrapper[**P](callback: Callable[Concatenate[int, P], None], *args: P.args, **kwargs: P.kwargs) -> None: ...
@overload
def callback(prefix: str, **options: Unpack[Config]) -> None: ...
@overload
def callback(prefix: int, **options: Unpack[Config]) -> None: ...
def callback(prefix: str | int, **options: Unpack[Config]) -> None: ...

wrapper(callback, value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:13:19
   |
13 | wrapper(callback, value="incorrect")  # snapshot: invalid-argument-type
   |                   ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
  --> src/mdtest_snippet.py:10:5
   |
10 | def callback(prefix: int, **options: Unpack[Config]) -> None: ...
   |     ^^^^^^^^              ------------------------- Parameter declared here
```

The same overload identity must survive `functools.partial`, which removes the bound prefix before
forwarding the remaining arguments.

```py
from functools import partial

def forward[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...

forward(partial(callback, 1), value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `forward` is incorrect
  --> src/mdtest_snippet.py:18:31
   |
18 | forward(partial(callback, 1), value="incorrect")  # snapshot: invalid-argument-type
   |                               ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
  --> src/mdtest_snippet.py:10:5
   |
10 | def callback(prefix: int, **options: Unpack[Config]) -> None: ...
   |     ^^^^^^^^              ------------------------- Parameter declared here
```

## Expanded positional parameters

`Unpack[tuple[int, str]]` creates two positional parameters from one `*values` declaration. An error
in the second argument should point to that declaration.

```py
from typing import Callable, Unpack

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
def callback(*values: Unpack[tuple[int, str]]) -> None: ...

wrapper(callback, 1, 2)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:6:22
  |
6 | wrapper(callback, 1, 2)  # snapshot: invalid-argument-type
  |                      ^ Expected `str`, found `Literal[2]`
info: Function defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def callback(*values: Unpack[tuple[int, str]]) -> None: ...
  |     ^^^^^^^^ -------------------------------- Parameter declared here
```

## Callback protocols

Unlike a plain `Callable` annotation, a callback protocol includes a declaration for `__call__`. The
diagnostic should point to the parameter on that method.

```py
from typing import Callable, Protocol

def wrapper[**P](callback: Callable[P, object], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Callback(Protocol):
    def __call__(self, *, value: int) -> None: ...

def run(callback: Callback) -> None:
    wrapper(callback, value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:9:23
  |
9 |     wrapper(callback, value="incorrect")  # snapshot: invalid-argument-type
  |                       ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Method defined here
 --> src/mdtest_snippet.py:6:9
  |
6 |     def __call__(self, *, value: int) -> None: ...
  |         ^^^^^^^^          ---------- Parameter declared here
```

## Callable objects

A callable object declares its accepted arguments on `__call__`. Point to that method when the
object is passed to a forwarding function.

```py
from typing import Callable

def wrapper[**P](callback: Callable[P, object], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Callback:
    def __call__(self, *, value: int) -> None: ...

wrapper(Callback(), value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:8:21
  |
8 | wrapper(Callback(), value="incorrect")  # snapshot: invalid-argument-type
  |                     ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Method defined here
 --> src/mdtest_snippet.py:6:9
  |
6 |     def __call__(self, *, value: int) -> None: ...
  |         ^^^^^^^^          ---------- Parameter declared here
```

## Constructors defined by __init__

A class passed as the callback receives the forwarded arguments in its constructor. A class that
declares `__init__` should identify the matching constructor parameter.

```py
from typing import Callable

def wrapper[**P](callback: Callable[P, object], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Factory:
    def __init__(self, value: int) -> None: ...

wrapper(Factory, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:8:18
  |
8 | wrapper(Factory, "incorrect")  # snapshot: invalid-argument-type
  |                  ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Method defined here
 --> src/mdtest_snippet.py:6:9
  |
6 |     def __init__(self, value: int) -> None: ...
  |         ^^^^^^^^       ---------- Parameter declared here
```

## Overloaded constructors defined by __init__

Synthesized constructor signatures should preserve the overload selected by `Concatenate`, even when
both overloads unpack the same `TypedDict` fields.

```py
from typing import Callable, Concatenate, TypedDict, Unpack, overload

class Options(TypedDict):
    value: int

def wrapper[**P, T](callback: Callable[Concatenate[int, P], T], *args: P.args, **kwargs: P.kwargs) -> T:
    return callback(1, *args, **kwargs)

class Factory:
    @overload
    def __init__(self, prefix: str, **options: Unpack[Options]) -> None: ...
    @overload
    def __init__(self, prefix: int, **options: Unpack[Options]) -> None: ...
    def __init__(self, prefix: str | int, **options: Unpack[Options]) -> None: ...

wrapper(Factory, value="incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:16:18
   |
16 | wrapper(Factory, value="incorrect")  # snapshot: invalid-argument-type
   |                  ^^^^^^^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Method defined here
  --> src/mdtest_snippet.py:13:9
   |
13 |     def __init__(self, prefix: int, **options: Unpack[Options]) -> None: ...
   |         ^^^^^^^^                    -------------------------- Parameter declared here
```

## Constructors defined by a metaclass

A custom metaclass can determine the accepted constructor arguments through its own `__call__`. That
declaration takes precedence over the class's `__init__` method.

```py
from typing import Callable

def wrapper[**P](callback: Callable[P, object], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Meta(type):
    def __call__(cls, value: int) -> object:
        return object()

class Factory(metaclass=Meta):
    def __init__(self, value: str) -> None: ...

wrapper(Factory, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:12:18
   |
12 | wrapper(Factory, "incorrect")  # snapshot: invalid-argument-type
   |                  ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Method defined here
 --> src/mdtest_snippet.py:6:9
  |
6 |     def __call__(cls, value: int) -> object:
  |         ^^^^^^^^      ---------- Parameter declared here
```

## Constructors defined by __new__

A constructor defined by `__new__` consumes `cls` before checking the forwarded arguments. The
diagnostic should point to `value`, not `cls`.

```py
from typing import Callable, Self

def wrapper[**P](callback: Callable[P, object], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Factory:
    def __new__(cls, value: int) -> Self:
        return super().__new__(cls)

wrapper(Factory, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:9:18
  |
9 | wrapper(Factory, "incorrect")  # snapshot: invalid-argument-type
  |                  ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:6:9
  |
6 |     def __new__(cls, value: int) -> Self:
  |         ^^^^^^^      ---------- Parameter declared here
```

## Overloaded constructors defined by __new__

When `__new__` is overloaded, the diagnostic must both select the matching overload and account for
its `cls` parameter.

```py
from typing import Callable, Self, overload

def wrapper[**P](callback: Callable[P, object], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Factory:
    @overload
    def __new__(cls, value: str) -> Self: ...
    @overload
    def __new__(cls, value: int, flag: int) -> Self: ...
    def __new__(cls, value: str | int, flag: int | None = None) -> Self:
        return super().__new__(cls)

wrapper(Factory, 1, "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:13:21
   |
13 | wrapper(Factory, 1, "incorrect")  # snapshot: invalid-argument-type
   |                     ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:9:9
  |
9 |     def __new__(cls, value: int, flag: int) -> Self: ...
  |         ^^^^^^^                  --------- Parameter declared here
```

## Functions wrapped by functools.partial

`functools.partial` supplies the first argument before the callback is passed to the forwarding
function. An invalid forwarded argument should point to the next parameter on the original function.

```py
from functools import partial
from typing import Callable

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
def callback(prefix: int, value: int) -> None: ...

wrapper(partial(callback, 1), "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
 --> src/mdtest_snippet.py:7:31
  |
7 | wrapper(partial(callback, 1), "incorrect")  # snapshot: invalid-argument-type
  |                               ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Function defined here
 --> src/mdtest_snippet.py:5:5
  |
5 | def callback(prefix: int, value: int) -> None: ...
  |     ^^^^^^^^              ---------- Parameter declared here
```

## Bound methods wrapped by functools.partial

When `functools.partial` wraps a bound method, both `self` and the argument supplied by `partial`
come before the forwarded argument.

```py
from functools import partial
from typing import Callable

def wrapper[**P](callback: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...

class Handler:
    def callback(self, prefix: int, value: int) -> None: ...

def run(handler: Handler) -> None:
    wrapper(partial(handler.callback, 1), "incorrect")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `wrapper` is incorrect
  --> src/mdtest_snippet.py:10:43
   |
10 |     wrapper(partial(handler.callback, 1), "incorrect")  # snapshot: invalid-argument-type
   |                                           ^^^^^^^^^^^ Expected `int`, found `Literal["incorrect"]`
info: Method defined here
 --> src/mdtest_snippet.py:7:9
  |
7 |     def callback(self, prefix: int, value: int) -> None: ...
  |         ^^^^^^^^                    ---------- Parameter declared here
```
