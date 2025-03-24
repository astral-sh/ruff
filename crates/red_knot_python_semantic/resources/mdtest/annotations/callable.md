# Callable

References:

- <https://typing.readthedocs.io/en/latest/spec/callables.html#callable>

Note that `typing.Callable` is deprecated at runtime, in favour of `collections.abc.Callable` (see:
<https://docs.python.org/3/library/typing.html#deprecated-aliases>). However, removal of
`typing.Callable` is not currently planned, and the canonical location of the stub for the symbol in
typeshed is still `typing.pyi`.

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

# error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
def _(c: Callable[int, str]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

Or, when it's a literal type:

```py
# error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
def _(c: Callable[42, str]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

Or, when one of the parameter type is invalid in the list:

```py
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
# error: [invalid-type-form] "Boolean literals are not allowed in this context in a type expression"
def _(c: Callable[[int, 42, str, False], None]):
    # revealed: (int, Unknown, str, Unknown, /) -> None
    reveal_type(c)
```

### Missing return type

Using a parameter list:

```py
from typing import Callable

# error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
def _(c: Callable[[int, str]]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

Or, an ellipsis:

```py
# error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
def _(c: Callable[...]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

Or something else that's invalid in a type expression generally:

```py
# fmt: off

def _(c: Callable[  # error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
            {1, 2}  # error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
        ]
    ):
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

### List as the second argument

```py
from typing import Callable

# fmt: off

def _(c: Callable[
            int,  # error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
            [str]  # error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
        ]
    ):
    reveal_type(c)  # revealed: (...) -> Unknown
```

### List as both arguments

```py
from typing import Callable

# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
def _(c: Callable[[int], [str]]):
    reveal_type(c)  # revealed: (int, /) -> Unknown
```

### Three list arguments

```py
from typing import Callable

# fmt: off


def _(c: Callable[  # error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
            [int],
            [str],  # error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
            [bytes]  # error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
        ]
    ):
    reveal_type(c)  # revealed: (...) -> Unknown
```

## Simple

A simple `Callable` with multiple parameters and a return type:

```py
from typing import Callable

def _(c: Callable[[int, str], int]):
    reveal_type(c)  # revealed: (int, str, /) -> int
```

## Union

```py
from typing import Callable, Union

def _(
    c: Callable[[Union[int, str]], int] | None,
    d: None | Callable[[Union[int, str]], int],
    e: None | Callable[[Union[int, str]], int] | int,
):
    reveal_type(c)  # revealed: ((int | str, /) -> int) | None
    reveal_type(d)  # revealed: None | ((int | str, /) -> int)
    reveal_type(e)  # revealed: None | ((int | str, /) -> int) | int
```

## Intersection

```py
from typing import Callable, Union
from knot_extensions import Intersection, Not

def _(
    c: Intersection[Callable[[Union[int, str]], int], int],
    d: Intersection[int, Callable[[Union[int, str]], int]],
    e: Intersection[int, Callable[[Union[int, str]], int], str],
    f: Intersection[Not[Callable[[int, str], Intersection[int, str]]]],
):
    reveal_type(c)  # revealed: ((int | str, /) -> int) & int
    reveal_type(d)  # revealed: int & ((int | str, /) -> int)
    reveal_type(e)  # revealed: int & ((int | str, /) -> int) & str
    reveal_type(f)  # revealed: ~((int, str, /) -> int & str)
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

## Member lookup

```py
from typing import Callable

def _(c: Callable[[int], int]):
    reveal_type(c.__init__)  # revealed: Literal[__init__]
    reveal_type(c.__class__)  # revealed: type

    # TODO: The member lookup for `Callable` uses `object` which does not have a `__call__`
    # attribute. We could special case `__call__` in this context. Refer to
    # https://github.com/astral-sh/ruff/pull/16493#discussion_r1985098508 for more details.
    # error: [unresolved-attribute] "Type `(int, /) -> int` has no attribute `__call__`"
    reveal_type(c.__call__)  # revealed: Unknown
```

[gradual form]: https://typing.readthedocs.io/en/latest/spec/glossary.html#term-gradual-form
