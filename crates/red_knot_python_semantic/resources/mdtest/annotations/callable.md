# Callable

References:

- <https://typing.readthedocs.io/en/latest/spec/callables.html#callable>

TODO: Use `collections.abc` as importing from `typing` is deprecated but this requires support for
`*` imports. See: <https://docs.python.org/3/library/typing.html#deprecated-aliases>.

## Invalid forms

The `Callable` special form requires _exactly_ two arguments where the first argument is either a
parameter type list, parameter specification, `typing.Concatenate`, or `...` and the second argument
is the return type. Here, we explore various invalid forms.

### Empty

A bare `Callable` without any type arguments:

```py
from typing import Callable

def _(c: Callable):
    reveal_type(c)  # revealed: (...) -> Unknown
```

### Invalid parameter type argument

When it's not a list:

```py
from typing import Callable

# error: [invalid-type-form] "The first argument to `typing.Callable` must be either a list of types, parameter specification, `typing.Concatenate`, or `...`"
def _(c: Callable[int, str]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

Or, when it's a literal type:

```py
# error: [invalid-type-form] "The first argument to `typing.Callable` must be either a list of types, parameter specification, `typing.Concatenate`, or `...`"
def _(c: Callable[42, str]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

Or, when one of the parameter type is invalid in the list:

```py
def _(c: Callable[[int, 42, str, False], None]):
    # revealed: (int, @Todo(number literal in type expression), str, @Todo(boolean literal in type expression), /) -> None
    reveal_type(c)
```

### Missing return type

Using a parameter list:

```py
from typing import Callable

# error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
def _(c: Callable[[int, str]]):
    reveal_type(c)  # revealed: (int, str, /) -> Unknown
```

Or, an ellipsis:

```py
# error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
def _(c: Callable[...]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

### More than two arguments

We can't reliably infer the callable type if there are more then 2 arguments because we don't know
which argument corresponds to either the parameters or the return type.

```py
from typing import Callable

# error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
def _(c: Callable[[int], str, str]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

## Simple

A simple `Callable` with multiple parameters and a return type:

```py
from typing import Callable

def _(c: Callable[[int, str], int]):
    reveal_type(c)  # revealed: (int, str, /) -> int
```

## Nested

A nested `Callable` as one of the parameter types:

```py
from typing import Callable

def _(c: Callable[[Callable[[int], str]], int]):
    reveal_type(c)  # revealed: ((int, /) -> str, /) -> int
```

And, as the return type:

```py
def _(c: Callable[[int, str], Callable[[int], int]]):
    reveal_type(c)  # revealed: (int, str, /) -> (int, /) -> int
```

## Gradual form

The `Callable` special form supports the use of `...` in place of the list of parameter types. This
is a [gradual form] indicating that the type is consistent with any input signature:

```py
from typing import Callable

def gradual_form(c: Callable[..., str]):
    reveal_type(c)  # revealed: (...) -> str

# TODO: This doesn't work yet as it requires understanding `lambda` expressions but it doesn't raise
# any errors because it's a `todo` type.
gradual_form(lambda: "hello")
gradual_form(lambda x: "hello")
gradual_form(lambda x, y: "hello")
```

## Using `typing.Concatenate`

Using `Concatenate` as the first argument to `Callable`:

```py
from typing_extensions import Callable, Concatenate

def _(c: Callable[Concatenate[int, str, ...], int]):
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> int
```

And, as one of the parameter types:

```py
def _(c: Callable[[Concatenate[int, str, ...], int], int]):
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> int
```

## Using `typing.ParamSpec`

Using a `ParamSpec` in a `Callable` annotation:

```py
from typing_extensions import Callable

# TODO: Not an error; remove once `ParamSpec` is supported
# error: [invalid-type-form]
def _[**P1](c: Callable[P1, int]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

And, using the legacy syntax:

```py
from typing_extensions import ParamSpec

P2 = ParamSpec("P2")

# TODO: Not an error; remove once `ParamSpec` is supported
# error: [invalid-type-form]
def _(c: Callable[P2, int]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

## Using `typing.Unpack`

Using the unpack operator (`*`):

```py
from typing_extensions import Callable, TypeVarTuple

Ts = TypeVarTuple("Ts")

def _(c: Callable[[int, *Ts], int]):
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> int
```

And, using the legacy syntax using `Unpack`:

```py
from typing_extensions import Unpack

def _(c: Callable[[int, Unpack[Ts]], int]):
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> int
```

[gradual form]: https://typing.readthedocs.io/en/latest/spec/glossary.html#term-gradual-form
