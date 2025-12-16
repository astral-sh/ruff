# Callable

References:

- <https://typing.python.org/en/latest/spec/callables.html#callable>

Note that `typing.Callable` is deprecated at runtime, in favor of `collections.abc.Callable` (see:
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

### Invalid parameters and return type

```py
from typing import Callable

# fmt: off

def _(c: Callable[
            # error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
            {1, 2}, 2  # error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
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

### Tuple as the second argument

```py
from typing import Callable

# fmt: off

def _(c: Callable[
            int,  # error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
            (str, )  # error: [invalid-type-form] "Tuple literals are not allowed in this context in a type expression"
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
from ty_extensions import Intersection, Not

class Foo: ...

def _(
    c: Intersection[Callable[[Union[int, str]], int], int],
    d: Intersection[int, Callable[[Union[int, str]], int]],
    e: Intersection[int, Callable[[Union[int, str]], int], Foo],
    f: Intersection[Not[Callable[[int, str], Intersection[int, Foo]]]],
):
    reveal_type(c)  # revealed: ((int | str, /) -> int) & int
    reveal_type(d)  # revealed: int & ((int | str, /) -> int)
    reveal_type(e)  # revealed: int & ((int | str, /) -> int) & Foo
    reveal_type(f)  # revealed: ~((int, str, /) -> int & Foo)
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
    # TODO: Should reveal the correct signature
    reveal_type(c)  # revealed: (...) -> int
```

And, as one of the parameter types:

```py
def _(c: Callable[[Concatenate[int, str, ...], int], int]):
    # TODO: Should reveal the correct signature
    reveal_type(c)  # revealed: (...) -> int
```

Other type expressions can be nested inside `Concatenate`:

```py
def _(c: Callable[[Concatenate[int | str, type[str], ...], int], int]):
    # TODO: Should reveal the correct signature
    reveal_type(c)  # revealed: (...) -> int
```

But providing fewer than 2 arguments to `Concatenate` is an error:

```py
# fmt: off

def _(
    c: Callable[Concatenate[int], int],  # error: [invalid-type-form] "Special form `typing.Concatenate` expected at least 2 parameters but got 1"
    d: Callable[Concatenate[(int,)], int],  # error: [invalid-type-form] "Special form `typing.Concatenate` expected at least 2 parameters but got 1"
    e: Callable[Concatenate[()], int]  # error: [invalid-type-form] "Special form `typing.Concatenate` expected at least 2 parameters but got 0"
):
    reveal_type(c)  # revealed: (...) -> int
    reveal_type(d)  # revealed: (...) -> int
    reveal_type(e)  # revealed: (...) -> int

# fmt: on
```

## Using `typing.ParamSpec`

```toml
[environment]
python-version = "3.12"
```

Using a `ParamSpec` in a `Callable` annotation:

```py
from typing_extensions import Callable

def _[**P1](c: Callable[P1, int]):
    reveal_type(P1.args)  # revealed: P1@_.args
    reveal_type(P1.kwargs)  # revealed: P1@_.kwargs

    reveal_type(c)  # revealed: (**P1@_) -> int
```

And, using the legacy syntax:

```py
from typing_extensions import ParamSpec

P2 = ParamSpec("P2")

def _(c: Callable[P2, int]):
    reveal_type(c)  # revealed: (**P2@_) -> int
```

## Using `typing.Unpack`

Using the unpack operator (`*`):

```py
from typing_extensions import Callable, TypeVarTuple

Ts = TypeVarTuple("Ts")

def _(c: Callable[[int, *Ts], int]):
    # TODO: Should reveal the correct signature
    reveal_type(c)  # revealed: (...) -> int
```

And, using the legacy syntax using `Unpack`:

```py
from typing_extensions import Unpack

def _(c: Callable[[int, Unpack[Ts]], int]):
    # TODO: Should reveal the correct signature
    reveal_type(c)  # revealed: (...) -> int
```

## Member lookup

```py
from typing import Callable

def _(c: Callable[[int], int]):
    reveal_type(c.__init__)  # revealed: bound method object.__init__() -> None
    reveal_type(c.__class__)  # revealed: type
    reveal_type(c.__call__)  # revealed: (int, /) -> int
```

Unlike other type checkers, we do _not_ allow attributes to be accessed that would only be available
on function-like callables:

```py
def f_wrong(c: Callable[[], None]):
    # error: [unresolved-attribute] "Object of type `() -> None` has no attribute `__qualname__`"
    c.__qualname__

    # error: [unresolved-attribute] "Unresolved attribute `__qualname__` on type `() -> None`."
    c.__qualname__ = "my_callable"
```

We do this, because at runtime, calls to `f_wrong` with a non-function callable would raise an
`AttributeError`:

```py
class MyCallable:
    def __call__(self) -> None:
        pass

f_wrong(MyCallable())  # raises `AttributeError` at runtime
```

If users want to read/write to attributes such as `__qualname__`, they need to check the existence
of the attribute first:

```py
from inspect import getattr_static

def f_okay(c: Callable[[], None]):
    if hasattr(c, "__qualname__"):
        reveal_type(c.__qualname__)  # revealed: object

        # TODO: should be `property`
        # (or complain that we don't know that `type(c)` has the attribute at all!)
        reveal_type(type(c).__qualname__)  # revealed: @Todo(Intersection meta-type)

        # `hasattr` only guarantees that an attribute is readable.
        #
        # error: [invalid-assignment] "Object of type `Literal["my_callable"]` is not assignable to attribute `__qualname__` on type `(() -> None) & <Protocol with members '__qualname__'>`"
        c.__qualname__ = "my_callable"

        result = getattr_static(c, "__qualname__")
        reveal_type(result)  # revealed: property
        if isinstance(result, property) and result.fset:
            c.__qualname__ = "my_callable"  # okay
```

## From a class

### Subclasses should return themselves, not superclass

```py
from ty_extensions import into_callable

class Base:
    def __init__(self) -> None:
        pass

class A(Base):
    pass

# revealed: () -> A
reveal_type(into_callable(A))
```

[gradual form]: https://typing.python.org/en/latest/spec/glossary.html#term-gradual-form
