## What it does

Checks whether a dataclass's `__post_init__` method accepts the arguments passed by the generated
`__init__` method.

## Why is this bad?

The generated `__init__` passes every `InitVar` field to `__post_init__` as a positional argument,
in field order. If the number or kind of parameters is incompatible with that call, constructing an
instance raises `TypeError`. If a parameter has an incompatible type, the method can receive a value
it cannot handle.

## Example

```python
from dataclasses import InitVar, dataclass


@dataclass
class Sample:
    count: InitVar[int]

    # error: [invalid-dataclass-post-init]
    def __post_init__(self, count: str) -> None: ...
```

Here, annotating the `count` parameter as `int` allows the method to accept the value passed by
`__init__`.

## Rule history

- v0.0.102: Split out from `invalid-dataclass`.
