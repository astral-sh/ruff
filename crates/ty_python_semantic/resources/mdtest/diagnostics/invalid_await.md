# Invalid await diagnostics

<!-- snapshot-diagnostics -->

## Basic

This is a test showcasing a primitive case where an object is not awaitable.

```py
async def main() -> None:
    await 1  # error: [invalid-await]
```

## Custom type with missing `__await__`

This diagnostic also points to the class definition if available.

```py
class MissingAwait:
    pass

async def main() -> None:
    await MissingAwait()  # error: [invalid-await]
```

## Custom type with possibly missing `__await__`

This diagnostic also points to the method definition if available.

```py
from datetime import datetime

class PossiblyUnbound:
    if datetime.today().weekday() == 0:
        def __await__(self):
            yield

async def main() -> None:
    await PossiblyUnbound()  # error: [invalid-await]
```

## `__await__` definition with extra arguments

Currently, the signature of `__await__` isn't checked for conformity with the `Awaitable` protocol
directly. Instead, individual anomalies are reported, such as the following. Here, the diagnostic
reports that the object is not implicitly awaitable, while also pointing at the function parameters.

```py
class InvalidAwaitArgs:
    def __await__(self, value: int):
        yield value

async def main() -> None:
    await InvalidAwaitArgs()  # error: [invalid-await]
```

## Non-callable `__await__`

This diagnostic points at the binding site of `__await__` on the user's class, rather than at the
definition of the attribute's type (see <https://github.com/astral-sh/ty/issues/3250>).

```py
class NonCallableAwait:
    __await__ = 42

async def main() -> None:
    await NonCallableAwait()  # error: [invalid-await]
```

If `__await__` is inherited from a base class, the diagnostic follows the MRO and points at the base
class's assignment.

```py
class BaseWithBadAwait:
    __await__ = 42

class InheritedNonCallableAwait(BaseWithBadAwait): ...

async def main() -> None:
    await InheritedNonCallableAwait()  # error: [invalid-await]
```

## `__await__` definition with explicit invalid return type

`__await__` must return a valid iterator. This diagnostic also points to the method definition if
available.

```py
class InvalidAwaitReturn:
    def __await__(self) -> int:
        return 5

async def main() -> None:
    await InvalidAwaitReturn()  # error: [invalid-await]
```

## Invalid union return type

When multiple potential definitions of `__await__` exist, all of them must be proper in order for an
instance to be awaitable. In this specific case, no specific function definition is highlighted.

```py
import typing
from datetime import datetime

class UnawaitableUnion:
    if datetime.today().weekday() == 6:
        def __await__(self) -> typing.Generator[typing.Any, None, None]:
            yield

    else:
        def __await__(self) -> int:
            return 5

async def main() -> None:
    await UnawaitableUnion()  # error: [invalid-await]
```
