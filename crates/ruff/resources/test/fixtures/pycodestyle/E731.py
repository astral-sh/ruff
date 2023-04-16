#: E731
f = lambda x: 2 * x
#: E731
f = lambda x: 2 * x
#: E731
while False:
    this = lambda y, z: 2 * x
#: E731
f = lambda: (yield 1)
#: E731
f = lambda: (yield from g())
#: E731
class F:
    f = lambda x: 2 * x


f = object()
f.method = lambda: "Method"
f = {}
f["a"] = lambda x: x**2
f = []
f.append(lambda x: x**2)
f = g = lambda x: x**2
lambda: "no-op"

# Annotated
from typing import Callable, ParamSpec

P = ParamSpec("P")

# ParamSpec cannot be used in this context, so do not preserve the annotation.
f: Callable[P, int] = lambda *args: len(args)
f: Callable[[], None] = lambda: None
f: Callable[..., None] = lambda a, b: None
f: Callable[[int], int] = lambda x: 2 * x

# Let's use the `Callable` type from `collections.abc` instead.
from collections.abc import Callable

f: Callable[[str, int], str] = lambda a, b: a * b
f: Callable[[str, int], tuple[str, int]] = lambda a, b: (a, b)
f: Callable[[str, int, list[str]], list[str]] = lambda a, b, /, c: [*c, a * b]


# Override `Callable`
class Callable:
    pass


# Do not copy the annotation from here on out.
f: Callable[[str, int], str] = lambda a, b: a * b
