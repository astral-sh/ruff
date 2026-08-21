## What it does

Checks for protocol classes that are invalid at runtime or do not satisfy the typing
specification.

## Why is this bad?

An invalidly defined protocol class may lead to the type checker inferring
unexpected things or accepting unsafe operations. Some invalid protocol definitions
also raise `TypeError` at runtime.

## Examples

A `Protocol` class cannot inherit from a non-`Protocol` class;
this raises a `TypeError` at runtime:

```pycon
>>> from typing import Protocol
>>> class Foo(int, Protocol): ...
Traceback (most recent call last):
  File "<python-input-1>", line 1, in <module>
    class Foo(int, Protocol): ...
TypeError: Protocols can only inherit from other protocols, got <class 'int'>
```

A generic protocol's declared type-variable variance must match how that variable is
used by its protocol members. For example, a type variable that appears only in a
method's return type must be covariant:

```py
from typing import Protocol, TypeVar

T = TypeVar("T")


class Source(Protocol[T]):  # error: [invalid-protocol]
    def read(self) -> T: ...
```

Although Python constructs this protocol successfully at runtime, it is invalid for
static typing. Declare the type variable with `TypeVar("T", covariant=True)` instead.
