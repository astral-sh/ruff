## What it does

Checks whether the declared variance of a type variable is compatible with how a generic class uses
it in its methods and base classes. In a generic protocol, the declared variance must match the
variance inferred from its interface.

## Why is this bad?

A covariant class allows a value of type `Box[int]` to be used where `Box[object]` is expected. If a
method could then store any `object` in that box, callers reading from the original `Box[int]` could
receive a value of the wrong type.

## Examples

```python
from typing import Generic, TypeVar

T_co = TypeVar("T_co", covariant=True)


class Box(Generic[T_co]):
    def put(self, value: T_co) -> None: ...  # error: [invalid-type-variable-variance]
```

The protocol below returns `T` directly from its only method, so `T` must be covariant.

```py
from typing import Protocol, TypeVar

T = TypeVar("T")


class Source(Protocol[T]):  # error: [invalid-type-variable-variance]
    def read(self) -> T: ...
```

Declare the protocol’s type variable with `TypeVar("T", covariant=True)` instead.

## Rule history

- v0.0.102: Split out from `invalid-generic-class` and `invalid-protocol`.
