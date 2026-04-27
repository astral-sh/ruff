# Derived constraint cycles

```toml
[environment]
python-version = "3.13"
```

Before [ty#24660], this example would never complete, because we would repeatedly try to substitute
one of the typevars in a constraint over and over, creating increasingly large types in the lower or
upper bound of the constraint.

```py
from typing import Callable, Protocol

class Foo[In, Out](Protocol):
    def method(self, other: In, /) -> Out:
        raise NotImplementedError

def add[In, Out](a: Foo[In, Out], b: In, /) -> Out:
    raise NotImplementedError

def reduce[T](function: Callable[[T, T], T]) -> T:
    raise NotImplementedError

reduce(add)
```

[ty#24660]: https://github.com/astral-sh/ruff/pull/24660
