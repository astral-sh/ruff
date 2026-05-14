# Derived constraint cycles

```toml
[environment]
python-version = "3.13"
```

## Initial repeated-substitution cycle

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

## Repeated substitutions across derived constraints

The repeat-guard introduced in [ty#24660] only suppressed a follow-up substitution when the second
attempt was into the same constraint id it had already substituted into. Every substitution produces
a new derived constraint, so chains that alternate between two typevars as each other's bounds kept
generating ever-deeper replacement types — for instance by alternating between substituting
`T1 → Iterable[T2]` and `T2 → T1` into the upper bound of a third constraint, each round adding
another `Iterable[...]` layer.

Keying the repeat-guard by the constrained typevar (which stays stable across the chain) caps each
substitution shape to at most one application per BDD path, preventing unbounded growth.

This pattern shows up in real code via `itertools.accumulate` combined with a builtin like `min`,
whose overloaded signature provides the cross-typevar constraints that drive the chain:

```py
from itertools import accumulate

def running_min(iterable):
    iterator = iter(iterable)
    return accumulate(iterator, func=min)
```

This is a version that more exaggerates the performance degradation. Even in the release build, it
took tens of seconds to complete.

```py
from itertools import accumulate

def nested_running_min(iterable):
    it = iter(iterable)
    return accumulate(
        accumulate(
            accumulate(accumulate(accumulate(it, func=min), func=min), func=min),
            func=min,
        ),
        func=min,
    )
```

[ty#24660]: https://github.com/astral-sh/ruff/pull/24660
