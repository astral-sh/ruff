## What it does

Reports invalid runtime checks against `Protocol` classes.
This includes explicit calls `isinstance()`/`issubclass()` against
non-runtime-checkable protocols, `issubclass()` calls against protocols
that have non-method members, and implicit `isinstance()` checks against
non-runtime-checkable protocols via pattern matching.

## Why is this bad?

These calls (implicit or explicit) raise `TypeError` at runtime.

## Examples

```python
from typing_extensions import Protocol, runtime_checkable


class HasX(Protocol):
    x: int


@runtime_checkable
class HasY(Protocol):
    y: int


def f(arg: object, arg2: type):
    # not runtime-checkable
    isinstance(arg, HasX)  # error: [isinstance-against-protocol]
    # not runtime-checkable
    issubclass(arg2, HasX)  # error: [isinstance-against-protocol]


def g(arg: object):
    match arg:
        # not runtime-checkable
        case HasX():  # error: [isinstance-against-protocol]
            pass


def h(arg2: type):
    isinstance(arg2, HasY)  # fine (runtime-checkable)

    # `HasY` is runtime-checkable, but has non-method members,
    # so it still can't be used in `issubclass` checks)
    issubclass(arg2, HasY)  # error: [isinstance-against-protocol]
```

## References

- [Typing documentation: `@runtime_checkable`](https://docs.python.org/3/library/typing.html#typing.runtime_checkable)
