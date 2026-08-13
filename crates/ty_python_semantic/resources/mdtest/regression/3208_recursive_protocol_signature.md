# Recursive protocol signature comparisons

This is a regression test for <https://github.com/astral-sh/ty/issues/3208>. Like the recursive
protocol test for growing attribute specializations in `protocols.md`, the input type parameter
grows with each recursive step. Here, the recursion goes through method signatures on an object that
satisfies the protocol structurally.

```toml
[environment]
python-version = "3.12"
```

`pkg/config.py`:

```py
from typing import Protocol

class Tagged[T]: ...
class Wrapped[T]: ...

class Impl[I, O]:
    def run(self, input: I) -> O:
        raise NotImplementedError

class Factory[I, O](Protocol):
    def create(self) -> Impl[I, O]: ...
    def tag(self) -> "Factory[Tagged[I], O]":
        from pkg.wrapper import TaggedFactory

        return TaggedFactory(self)

    def wrap(self) -> "Factory[Wrapped[I], O]": ...
```

`pkg/wrapper.py`:

```py
from pkg.config import Factory, Impl, Tagged, Wrapped

class TaggedFactory[I, O]:
    def __init__(self, inner: Factory[I, O]) -> None: ...
    def create(self) -> Impl[Tagged[I], O]:
        raise NotImplementedError

    def tag(self) -> Factory[Tagged[Tagged[I]], O]:
        raise NotImplementedError

    def wrap(self) -> Factory[Wrapped[Tagged[I]], O]:
        raise NotImplementedError
```
