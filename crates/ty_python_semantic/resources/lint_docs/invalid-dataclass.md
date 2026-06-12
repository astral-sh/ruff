## What it does

Checks for invalid applications of the `@dataclass` decorator.

## Why is this bad?

Applying `@dataclass` to a class that inherits from `NamedTuple`, `TypedDict`,
`Enum`, or `Protocol` is invalid:

- `NamedTuple` and `TypedDict` classes will raise an exception at runtime when
    instantiating the class.
- `Enum` classes with `@dataclass` are [explicitly not supported].
- `Protocol` classes define interfaces and cannot be instantiated.

## Examples

```python
from dataclasses import dataclass
from typing import NamedTuple


@dataclass
class Foo(NamedTuple):  # error: [invalid-dataclass]
    x: int
```

[explicitly not supported]: https://docs.python.org/3/howto/enum.html#dataclass-support
