## What it does

Detects errors in `TypedDict` class headers, such as unexpected arguments
or invalid base classes.

## Why is this bad?

The typing spec states that `TypedDict`s are not permitted to have
custom metaclasses. Using `**` unpacking in a `TypedDict` header
is also prohibited by ty, as it means that ty cannot statically determine
whether keys in the `TypedDict` are intended to be required or optional.

## Example

```python
from typing import TypedDict


class Meta(type): ...


class Foo(TypedDict, metaclass=Meta):  # error: [invalid-typed-dict-header]
    ...


def f(options: dict[str, object]):
    class Bar(TypedDict, **options):  # error: [invalid-typed-dict-header]
        ...
```
