## What it does

Checks for invalid applications of the `@dataclass` decorator.

## Why is this bad?

Applying `@dataclass` with incompatible arguments raises an exception while creating the class:

- `order=True` with `eq=False`
- `weakref_slot=True` with `slots=False`
- `slots=True` when the class already defines `__slots__`

Applying `@dataclass` to a class that inherits from `NamedTuple`, `TypedDict`, `Enum`, or `Protocol`
is also invalid:

- `NamedTuple` and `TypedDict` classes will raise an exception at runtime when instantiating the
    class.
- `Enum` classes with `@dataclass` are [explicitly not supported].
- `Protocol` classes define interfaces and cannot be instantiated.

## Examples

```python
from dataclasses import dataclass
from typing import NamedTuple


@dataclass(order=True, eq=False)  # error: [invalid-dataclass]
class Ordered: ...


@dataclass
class Foo(NamedTuple):  # error: [invalid-dataclass]
    x: int
```

See: <https://docs.python.org/3/library/dataclasses.html#dataclasses.dataclass>

## Rule history

- v0.0.102: Split out `invalid-dataclass-post-init`. To keep a custom rule level for incompatible
    `__post_init__` signatures, apply it to the new rule as well. Use `invalid-dataclass-post-init`
    in suppression comments for those signatures.

[explicitly not supported]: https://docs.python.org/3/howto/enum.html#dataclass-support
