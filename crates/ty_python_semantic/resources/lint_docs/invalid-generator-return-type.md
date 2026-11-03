## What it does

Checks whether a generator function's return annotation allows the generator object it produces.

## Why is this bad?

Calling a generator function produces a generator object. A synchronous generator function's
return-type annotation must be a supertype of `types.GeneratorType[Never, object, Never]`, and an
asynchronous generator function's return-type annotation must be a supertype of
`types.AsyncGeneratorType[Never, object]`. Other return-type annotations contradict what the
function does, so must be disallowed.

## Examples

```python
from collections.abc import Generator


# error: [invalid-generator-return-type]
def numbers() -> int:
    yield 1


# error: [invalid-generator-return-type]
async def numbers() -> Generator[int]:
    yield 1
```

Use instead:

```py
from collections.abc import AsyncGenerator, Generator


def numbers() -> Generator[int]:
    yield 1


async def numbers() -> AsyncGenerator[int]:
    yield 1
```
