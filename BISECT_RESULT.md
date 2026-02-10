# Bisect Result for ty#2759

## Issue

False positive `missing-argument` errors when using `ParamSpec` with `Concatenate`
in decorator patterns (e.g., `hypothesis.composite`).

https://github.com/astral-sh/ty/issues/2759

## First Bad Commit

```
4f684db775ef0816c37b19a99bbdccc00decf8cc
[ty] Detect generic `Callable`s in function signatures (#22954)
Author: Douglas Creager <dcreager@dcreager.net>
Date:   Tue Feb 3 20:41:55 2026 -0500
```

PR: https://github.com/astral-sh/ruff/pull/22954

## Minimal Reproducer

```python
from __future__ import annotations
from typing import Callable, Concatenate, ParamSpec, TypeVar

P = ParamSpec("P")
Ex = TypeVar("Ex")

class DrawFn:
    pass

class SearchStrategy:
    pass

def composite(
    f: Callable[Concatenate[DrawFn, P], Ex],
) -> Callable[P, SearchStrategy]:
    ...

@composite
def optional_ints(draw: DrawFn) -> int | None:
    return None

# P should be inferred as () here, so optional_ints() should be valid
x = optional_ints()  # error: missing-argument (false positive)
```

## Analysis

The commit introduces a heuristic: if a generic function binds a typevar that is
only mentioned in a `Callable` in return position, the callable (not the function)
is considered generic. This is intended for decorator factory patterns.

However, this heuristic appears to incorrectly apply to `ParamSpec` parameters
used with `Concatenate`. In the `composite` function, `P` appears in both:
- The parameter: `Callable[Concatenate[DrawFn, P], Ex]`
- The return type: `Callable[P, SearchStrategy]`

The heuristic may be incorrectly treating `P` in the return type's `Callable[P, ...]`
as a generic parameter of the returned callable, rather than resolving it from
the input function's signature via `Concatenate`.
