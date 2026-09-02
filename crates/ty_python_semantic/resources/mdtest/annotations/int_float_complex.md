# Special cases for int/float/complex in annotations

In order to support common use cases, an annotation of `float` actually means `int | float`, and an
annotation of `complex` actually means `int | float | complex`. See
[the specification](https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex)

## float

An annotation of `float` means `int | float`, so `int` is assignable to it:

```py
def takes_float(x: float):
    pass

def passes_int_to_float(x: int):
    # no error!
    takes_float(x)
```

It also applies to variable annotations:

```py
def assigns_int_to_float(x: int):
    # no error!
    y: float = x
```

It doesn't work the other way around:

```py
def takes_int(x: int):
    pass

def passes_float_to_int(x: float):
    # error: [invalid-argument-type]
    takes_int(x)

def assigns_float_to_int(x: float):
    # error: [invalid-assignment]
    y: int = x
```

Ty displays these numeric-tower unions using the canonical spellings `float` and `complex`. Exact
runtime instances are displayed as `float*` and `complex*` to preserve the distinction. The starred
spellings are only used in type displays; use `ty_extensions.JustFloat` or `JustComplex` to write
the exact types in annotations.

```py
def f(x: float):
    reveal_type(x)  # revealed: float

def returns_float() -> float:
    return 1

reveal_type(returns_float())  # revealed: float
reveal_type(1.0)  # revealed: float*
```

## complex

An annotation of `complex` means `int | float | complex`, so `int` and `float` are both assignable
to it (but not the other way around):

```py
def takes_complex(x: complex):
    pass

def passes_to_complex(x: float, y: int):
    # no errors!
    takes_complex(x)
    takes_complex(y)

def assigns_to_complex(x: float, y: int):
    # no errors!
    a: complex = x
    b: complex = y

def takes_int(x: int):
    pass

def takes_float(x: float):
    pass

def passes_complex(x: complex):
    # error: [invalid-argument-type]
    takes_int(x)
    # error: [invalid-argument-type]
    takes_float(x)

def assigns_complex(x: complex):
    # error: [invalid-assignment]
    y: int = x
    # error: [invalid-assignment]
    z: float = x

def f(x: complex):
    reveal_type(x)  # revealed: complex

reveal_type(1j)  # revealed: complex*
```

## Shadowed numeric builtins

Canonical numeric names remain qualified when a module defines a class with the same name:

```py
import builtins

class float: ...
class complex: ...

def reveal_shadowed_names(
    x: builtins.float | float,
    y: builtins.complex | complex,
):
    reveal_type(x)  # revealed: builtins.float | mdtest_snippet.float
    reveal_type(y)  # revealed: builtins.complex | mdtest_snippet.complex

def takes_custom_float(x: float): ...
def pass_builtin_float(x: builtins.float):
    # error: [invalid-argument-type] "Argument to function `takes_custom_float` is incorrect: Expected `mdtest_snippet.float`, found `builtins.float`"
    takes_custom_float(x)
```

## Narrowing

`int`, `float` and `complex` are all disjoint, which means that the union `int | float` can easily
be narrowed to `int` or `float`:

```py
from typing_extensions import assert_type
from ty_extensions import JustFloat

def f(x: complex):
    reveal_type(x)  # revealed: complex

    if isinstance(x, int):
        reveal_type(x)  # revealed: int
    elif isinstance(x, float):
        reveal_type(x)  # revealed: float*
    else:
        reveal_type(x)  # revealed: complex*

    assert isinstance(x, float)
    assert_type(x, JustFloat)
```
