# `ParamSpec`

## Definition

### Valid

```py
from typing import ParamSpec

P = ParamSpec("P")
reveal_type(type(P))  # revealed: <class 'ParamSpec'>
reveal_type(P)  # revealed: typing.ParamSpec
reveal_type(P.__name__)  # revealed: Literal["P"]
```

The paramspec name can also be provided as a keyword argument:

```py
from typing import ParamSpec

P = ParamSpec(name="P")
reveal_type(P.__name__)  # revealed: Literal["P"]
```

### Must be directly assigned to a variable

```py
from typing import ParamSpec

P = ParamSpec("P")
# error: [invalid-paramspec]
P1: ParamSpec = ParamSpec("P1")

# error: [invalid-paramspec]
tuple_with_typevar = ("foo", ParamSpec("W"))
reveal_type(tuple_with_typevar[1])  # revealed: ParamSpec
```

```py
from typing_extensions import ParamSpec

T = ParamSpec("T")
# error: [invalid-paramspec]
P1: ParamSpec = ParamSpec("P1")

# error: [invalid-paramspec]
tuple_with_typevar = ("foo", ParamSpec("P2"))
reveal_type(tuple_with_typevar[1])  # revealed: ParamSpec
```

### `ParamSpec` parameter must match variable name

```py
from typing import ParamSpec

P1 = ParamSpec("P1")

# error: [invalid-paramspec]
P2 = ParamSpec("P3")
```

### Accepts only a single `name` argument

> The runtime should accept bounds and covariant and contravariant arguments in the declaration just
> as typing.TypeVar does, but for now we will defer the standardization of the semantics of those
> options to a later PEP.

```py
from typing import ParamSpec

# error: [invalid-paramspec]
P1 = ParamSpec("P1", bound=int)
# error: [invalid-paramspec]
P2 = ParamSpec("P2", int, str)
# error: [invalid-paramspec]
P3 = ParamSpec("P3", covariant=True)
# error: [invalid-paramspec]
P4 = ParamSpec("P4", contravariant=True)
```

### Defaults

```toml
[environment]
python-version = "3.13"
```

The default value for a `ParamSpec` can be either a list of types, `...`, or another `ParamSpec`.

```py
from typing import ParamSpec

P1 = ParamSpec("P1", default=[int, str])
P2 = ParamSpec("P2", default=...)
P3 = ParamSpec("P3", default=P2)
```

Other values are invalid.

```py
# error: [invalid-paramspec]
P4 = ParamSpec("P4", default=int)
```

### PEP 695

```toml
[environment]
python-version = "3.12"
```

#### Valid

```py
def foo1[**P]() -> None:
    reveal_type(P)  # revealed: typing.ParamSpec

def foo2[**P = ...]() -> None:
    reveal_type(P)  # revealed: typing.ParamSpec

def foo3[**P = [int, str]]() -> None:
    reveal_type(P)  # revealed: typing.ParamSpec

def foo4[**P, **Q = P]():
    reveal_type(P)  # revealed: typing.ParamSpec
    reveal_type(Q)  # revealed: typing.ParamSpec
```

#### Invalid

ParamSpec, when defined using the new syntax, does not allow defining bounds or constraints.

This results in a lot of syntax errors mainly because the AST doesn't accept them in this position.
The parser could do a better job in recovering from these errors.

<!-- blacken-docs:off -->

```py
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
def foo[**P: int]() -> None:
    # error: [invalid-syntax]
    # error: [invalid-syntax]
    pass
```

<!-- blacken-docs:on -->

#### Invalid default

```py
# error: [invalid-paramspec]
def foo[**P = int]() -> None:
    pass
```
