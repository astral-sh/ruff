# Generic `__iter__` receiver with `Iterable` upper bound

Regression test for <https://github.com/astral-sh/ty/issues/3598>.

```toml
[environment]
python-version = "3.12"
```

```py
from collections.abc import Iterable, Iterator

class Unpacker[T: Iterable[object]]:
    def __init__(self, it: T, /) -> None:
        self._it = it
    def __iter__[S](self: "Unpacker[Iterable[S]]") -> Iterator[S]:
        return iter(self._it)

def integers() -> Unpacker[Iterable[int]]:
    return Unpacker([1, 2, 3])

reveal_type(tuple(integers()))  # revealed: tuple[int, ...]
for x in integers():
    reveal_type(x)  # revealed: int
reveal_type(list(integers()))  # revealed: list[int]
reveal_type(iter(integers()))  # revealed: Iterator[int]
```
