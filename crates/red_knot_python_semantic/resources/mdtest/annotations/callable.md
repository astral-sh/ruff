# Callable

References:
* <https://typing.readthedocs.io/en/latest/spec/callables.html#callable>

## Invalid forms

The `Callable` special form requires _exactly_ two arguments where the first argument is either a
parameter type list, parameter specification, `typing.Concatenate`, or `...` and the second argument
is the return type. Here, we explore various invalid forms.

### Syntax error

```py
from typing import Callable

# error: [invalid-syntax] "Expected index or slice expression"
# error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
def _(c: Callable[]):
    # TODO: This should be `() -> Unknown` which requires understanding that an empty `Name` is not
    # a param spec variable
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> Unknown
```

### Empty

It is invalid to have a bare `Callable` without any arguments:

```py
from typing import Callable

# error: [invalid-type-form] "`Callable` requires exactly two arguments when used in a type expression"
def _(c: Callable):
    reveal_type(c)  # revealed: Unknown
```

### Invalid parameter type argument

When it's not a list:

```py
from typing import Callable

def _(c: Callable[int, str]):
    # TODO: This should be `() -> Unknown` which requires understanding that `int` is not a param spec variable
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> str
```

Or, when it's a literal type:

```py
# error: [invalid-type-form] "The first argument to `typing.Callable` must be either a list of types, parameter specification, `typing.Concatenate`, or `...`"
def _(c: Callable[42, str]):
    reveal_type(c)  # revealed: () -> Unknown
```

Or, when one of the parameter type is invalid in the list:

```py
# error: [invalid-type-form] "Invalid type `Literal[42]` in parameter list"
# error: [invalid-type-form] "Invalid type `Literal[False]` in parameter list"
def _(c: Callable[[int, 42, str, False], None]):
    reveal_type(c)  # revealed: (int, Unknown, str, Unknown, /) -> None
```

### Missing return type

Using a parameter list:

```py
from typing import Callable

# error: [invalid-type-form]
def _(c: Callable[[int, str]]):
    reveal_type(c)  # revealed: (int, str, /) -> Unknown
```

Or, an ellipsis:

```py
# error: [invalid-type-form]
def _(c: Callable[...]):
    reveal_type(c)  # revealed: (*args: Any, **kwargs: Any) -> Unknown
```

### More than two arguments

We can't reliably infer the callable type if there are more then 2 arguments because we don't know
which argument corresponds to either the parameters or the return type.

```py
from typing import Callable

# error: [invalid-type-form]
def _(c: Callable[[int], str, str]):
    reveal_type(c)  # revealed: Unknown
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

The `Callable` special form supports the use of `...` in place of the list of parameter types. This is a
[gradual form] indicating that the type is consistent with any input signature:

```py
from typing import Callable

def gradual_form(c: Callable[..., str]):
    reveal_type(c)  # revealed: (*args: Any, **kwargs: Any) -> str

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

Defining `ParamSpec` explicitly:

```py
from typing_extensions import Callable, ParamSpec

P1 = ParamSpec("P1")

def _(c: Callable[P1, int]):
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> int
```

And, defining it using generics:

```py
def _[**P2](c: Callable[P2, int]):
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> int
```

## Using `typing.Unpack`

Using an explicit `Unpack`:

```py
from typing_extensions import Callable, Unpack, TypeVarTuple

Ts = TypeVarTuple("Ts")

def _(c: Callable[[int, Unpack[Ts]], int]):
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> int
```

And, using the unpack operator (`*`):

```py
def _(c: Callable[[int, *Ts], int]):
    reveal_type(c)  # revealed: (*args: @Todo(todo signature *args), **kwargs: @Todo(todo signature **kwargs)) -> int
```

[gradual form]: https://typing.readthedocs.io/en/latest/spec/glossary.html#term-gradual-form
