# Unused awaitable

## Basic coroutine not awaited

Calling an `async def` function produces a coroutine. If that coroutine is discarded without being
awaited, ty reports an `unused-awaitable` diagnostic. Inside an asynchronous function, this
diagnostic includes an autofix that adds `await`. The fix is marked unsafe because awaiting the
coroutine executes its body and can suspend the calling function.

```py
async def fetch() -> int:
    return 42

async def fetch_complex(x) -> int:
    return 42

async def main():
    fetch()  # snapshot: unused-awaitable
    fetch_complex(lambda: None)  # error: [unused-awaitable]
```

```snapshot
warning[unused-awaitable]: Object of type `CoroutineType[Any, Any, int]` is not awaited
 --> src/mdtest_snippet.py:8:5
  |
8 |     fetch()  # snapshot: unused-awaitable
  |     ^^^^^^^
help: Did you mean to `await` this expression?
  |
7 | async def main():
  -     fetch()  # snapshot: unused-awaitable
8 +     await fetch()  # snapshot: unused-awaitable
9 |     fetch_complex(lambda: None)  # error: [unused-awaitable]
  |
note: This is an unsafe fix and may change runtime behavior
```

## Awaited coroutine is fine

```py
async def fetch() -> int:
    return 42

async def main():
    await fetch()
```

## Assigned coroutine is fine

```py
async def fetch() -> int:
    return 42

async def main():
    # TODO: ty should eventually warn about unused coroutines assigned to variables
    coro = fetch()
```

## Coroutine passed to a function

When a coroutine is passed as an argument rather than used as an expression statement, no diagnostic
should be emitted.

```py
async def fetch() -> int:
    return 42

async def main():
    print(fetch())
```

## Top-level coroutine call

The lint fires even outside of `async def`, since the coroutine is still discarded. No fix is
offered because Python modules do not allow top-level `await`.

```py
async def fetch() -> int:
    return 42

fetch()  # snapshot: unused-awaitable
```

```snapshot
warning[unused-awaitable]: Object of type `CoroutineType[Any, Any, int]` is not awaited
 --> src/mdtest_snippet.py:4:1
  |
4 | fetch()  # snapshot: unused-awaitable
  | ^^^^^^^
```

## Synchronous function nested in an asynchronous function

A synchronous function cannot use `await`, even when it is defined inside an asynchronous function.
This diagnostic therefore has no fix.

```py
async def fetch() -> int:
    return 42

async def main():
    def inner():
        fetch()  # snapshot: unused-awaitable
```

```snapshot
warning[unused-awaitable]: Object of type `CoroutineType[Any, Any, int]` is not awaited
 --> src/mdtest_snippet.py:6:9
  |
6 |         fetch()  # snapshot: unused-awaitable
  |         ^^^^^^^
```

## Class nested in an asynchronous function

A class body cannot use `await`, even when the class is defined inside an asynchronous function.
This diagnostic therefore has no fix.

```py
async def fetch() -> int:
    return 42

async def main():
    class Inner:
        fetch()  # snapshot: unused-awaitable
```

```snapshot
warning[unused-awaitable]: Object of type `CoroutineType[Any, Any, int]` is not awaited
 --> src/mdtest_snippet.py:6:9
  |
6 |         fetch()  # snapshot: unused-awaitable
  |         ^^^^^^^
```

## Coroutine variables

A fix is also offered when a coroutine variable is used as an expression statement.

```py
from types import CoroutineType
from typing import Any

async def main(value: CoroutineType[Any, Any, int]):
    value  # snapshot: unused-awaitable
```

```snapshot
warning[unused-awaitable]: Object of type `CoroutineType[Any, Any, int]` is not awaited
 --> src/mdtest_snippet.py:5:5
  |
5 |     value  # snapshot: unused-awaitable
  |     ^^^^^
help: Did you mean to `await` this expression?
  |
4 | async def main(value: CoroutineType[Any, Any, int]):
  -     value  # snapshot: unused-awaitable
5 +     await value  # snapshot: unused-awaitable
  |
note: This is an unsafe fix and may change runtime behavior
```

## Conditional expressions

When a conditional expression returns an unused coroutine, ty offers an autofix that awaits the
selected coroutine. The fix parenthesizes the expression so that `await` applies to either branch.
Existing parentheses and comments are preserved.

```py
async def fetch() -> int:
    return 42

async def main(flag: bool):
    (  # Keep the expression on multiple lines.
        fetch() if flag else fetch()  # snapshot: unused-awaitable
    )
```

```snapshot
warning[unused-awaitable]: Object of type `CoroutineType[Any, Any, int]` is not awaited
 --> src/mdtest_snippet.py:6:9
  |
6 |         fetch() if flag else fetch()  # snapshot: unused-awaitable
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Did you mean to `await` this expression?
  |
5 |     (  # Keep the expression on multiple lines.
  -         fetch() if flag else fetch()  # snapshot: unused-awaitable
6 +         await (fetch() if flag else fetch())  # snapshot: unused-awaitable
7 |     )
  |
note: This is an unsafe fix and may change runtime behavior
```

## Already-awaited expressions

Awaiting a coroutine can return another coroutine that is then discarded. In this case, ty reports
`unused-awaitable` and offers an autofix that awaits the returned coroutine. The fix parenthesizes
the existing `await` expression before adding the second `await`.

```py
from types import CoroutineType
from typing import Any

async def fetch() -> CoroutineType[Any, Any, int]:
    raise NotImplementedError

async def main():
    await fetch()  # snapshot: unused-awaitable
```

```snapshot
warning[unused-awaitable]: Object of type `CoroutineType[Any, Any, int]` is not awaited
 --> src/mdtest_snippet.py:8:5
  |
8 |     await fetch()  # snapshot: unused-awaitable
  |     ^^^^^^^^^^^^^
help: Did you mean to `await` this expression?
  |
7 | async def main():
  -     await fetch()  # snapshot: unused-awaitable
8 +     await (await fetch())  # snapshot: unused-awaitable
  |
note: This is an unsafe fix and may change runtime behavior
```

## Union of awaitables

When every element of a union is awaitable, the lint should fire.

```py
from types import CoroutineType
from typing import Any

def get_coroutine() -> CoroutineType[Any, Any, int] | CoroutineType[Any, Any, str]:
    raise NotImplementedError

async def main():
    get_coroutine()  # error: [unused-awaitable]
```

## Union with non-awaitable

When a union contains a non-awaitable element, the lint should not fire.

```py
from types import CoroutineType
from typing import Any

def get_maybe_coroutine() -> CoroutineType[Any, Any, int] | int:
    raise NotImplementedError

async def main():
    get_maybe_coroutine()
```

## Intersection with awaitable

When an intersection type contains an awaitable element, the lint should fire.

```py
from collections.abc import Coroutine
from types import CoroutineType
from ty_extensions import Intersection

class Foo: ...
class Bar: ...

def get_coroutine() -> Intersection[Coroutine[Foo, Foo, Foo], CoroutineType[Bar, Bar, Bar]]:
    raise NotImplementedError

async def main():
    get_coroutine()  # error: [unused-awaitable]
```

## `reveal_type` and `assert_type` are not flagged

Calls to `reveal_type` and `assert_type` should not trigger this lint, even when their argument is
an awaitable.

```py
from typing_extensions import assert_type
from types import CoroutineType
from typing import Any

async def fetch() -> int:
    return 42

async def main():
    reveal_type(fetch())  # revealed: CoroutineType[Any, Any, int]
    assert_type(fetch(), CoroutineType[Any, Any, int])
```

## Non-awaitable expression statement

Regular non-awaitable expression statements should not trigger this lint.

```py
def compute() -> int:
    return 42

def main():
    compute()
```

## Dynamic type

`Any` and `Unknown` types should not trigger the lint.

```py
from typing import Any

def get_any() -> Any:
    return None

async def main():
    get_any()
```

## Notebook cells

Notebook cells allow top-level `await`, so ty can offer an `await` autofix for unused coroutines at
module scope.

```ipynb
{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": [
        "async def fetch() -> int:\n",
        "    return 42\n",
        "\n",
        "fetch()  # snapshot: unused-awaitable\n"
      ]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```

```snapshot
warning[unused-awaitable]: Object of type `CoroutineType[Any, Any, int]` is not awaited
 --> src/mdtest_snippet.ipynb:cell 1:4:1
  |
4 | fetch()  # snapshot: unused-awaitable
  | ^^^^^^^
help: Did you mean to `await` this expression?
 ::: cell 1
  |
3 |
  - fetch()  # snapshot: unused-awaitable
4 + await fetch()  # snapshot: unused-awaitable
5 |
  |
note: This is an unsafe fix and may change runtime behavior
```
