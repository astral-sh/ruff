# Recursive callable TypeVar bound

```toml
[environment]
python-version = "3.13"
```

A `TypeVar` bound can reference a recursive callable alias. Using the generic class default must not
expand the callable alias indefinitely.

```pyi
from typing import Callable, Generic, TypeAlias, TypeVar

ElementCallable: TypeAlias = Callable[..., Element[ElementCallable]]
Tag = TypeVar("Tag", default=str, bound=str | ElementCallable)

class Element(Generic[Tag]):
    def __init__(self, tag: Tag) -> None: ...

def make() -> Element:
    return Element("svg")

reveal_type(make())  # revealed: Element[str]
```
