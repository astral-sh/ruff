## What it does

Checks for type parameters in an invalid order. Required parameters must precede parameters with
defaults, and a default can only refer to type parameters that precede it. A `TypeVar` with a
default cannot follow a `TypeVarTuple`.

## Why is this bad?

Type arguments are assigned to parameters by position. If a required parameter follows one with a
default, a caller cannot supply the required argument while omitting the optional one. After a
`TypeVarTuple`, it would be ambiguous whether a positional argument belongs to the tuple or to the
parameter with the default.

## Examples

```toml
[environment]
python-version = "3.13"
```

```python
from typing import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U", default=int)


class C(Generic[U, T]): ...  # error: [invalid-type-parameter-order]


type Alias[*Ts, V = int] = tuple[*Ts, V]  # error: [invalid-type-parameter-order]
```

A default can depend on another parameter in the same declaration only if that parameter comes
first. Here, both parameters have defaults, but `T2` depends on `T1`.

```python
T1 = TypeVar("T1", default=int)
T2 = TypeVar("T2", default=T1)


class ForwardReference(Generic[T2, T1]): ...  # error: [invalid-type-parameter-order]
```

Putting `T1` first, as in `Generic[T1, T2]`, makes its value available to `T2`.

Python itself rejects a required parameter after a default in a PEP 695 parameter list, such as
`class C[U = int, T]: ...`. ty reports that case as `invalid-syntax`.

## Rule history

- v0.0.102: Split out from `invalid-generic-class` and `invalid-type-variable-default`.
