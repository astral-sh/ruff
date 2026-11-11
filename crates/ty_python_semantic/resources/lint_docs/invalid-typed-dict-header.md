## What it does

Detects errors in `TypedDict` class headers, such as unexpected arguments or invalid base classes.

## Why is this bad?

The typing spec states that `TypedDict`s are not permitted to have custom metaclasses. Using `**`
unpacking in a `TypedDict` header is also prohibited by ty, as it means that ty cannot statically
determine whether keys in the `TypedDict` are intended to be required or optional.

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

## Rule history

- v0.0.102: Split out `incompatible-typed-dict-openness`. To keep a custom severity for inherited
    openness errors, set it on the new rule as well. Use the new name in ignore comments for those
    errors, keeping both names if the same line also has a malformed header.
