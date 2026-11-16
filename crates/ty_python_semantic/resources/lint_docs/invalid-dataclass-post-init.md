## What it does

Checks whether a dataclass's `__post_init__` method accepts the arguments passed by the generated
`__init__` method.

## Why is this bad?

If a dataclass defines a `__post_init__` method, its generated `__init__` method will pass every
`InitVar` field as a positional argument to the `__post_init__` method. If the number or kind of
parameters is incompatible with that call, constructing an instance raises `TypeError`. If a
parameter has an incompatible type, the method can receive a value it cannot handle.

## Example

```python
from dataclasses import InitVar, dataclass


@dataclass
class Sample:
    count: InitVar[int]

    # `count` is declared as `int` in the field body, but declared as `str` in `__post_init__`:
    #
    # error: [invalid-dataclass-post-init]
    def __post_init__(self, count: str) -> None: ...
```

## Rule history

- v0.0.105: Split out from `invalid-dataclass`.
