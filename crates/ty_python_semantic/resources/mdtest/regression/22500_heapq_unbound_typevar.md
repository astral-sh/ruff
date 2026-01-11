# Unconstrained typevars must satisfy heapq bounds

This is a regression test for <https://github.com/astral-sh/ruff/pull/22500>.

```toml
[environment]
python-version = "3.11"
```

```py
from heapq import heappush
from typing import Callable, TypeVar

from _typeshed import SupportsRichComparison
from ty_extensions import TypeOf, is_assignable_to, static_assert

T = TypeVar("T")

# `heappush` should only be assignable to callables that require a comparable typevar.
static_assert(
    not is_assignable_to(
        TypeOf[heappush],
        Callable[[list[T], T], None],
    )
)

static_assert(
    is_assignable_to(
        TypeOf[heappush],
        Callable[[list[SupportsRichComparison], SupportsRichComparison], None],
    )
)
```
