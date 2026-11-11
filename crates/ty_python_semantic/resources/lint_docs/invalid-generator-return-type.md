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

## See also

- `invalid-explicit-return` emits errors on synchronous generator functions where the return-type
    annotation is a valid supertype of `types.GeneratorType` but specific `return` statements in the
    body of the generator function are not consistent with the third type parameter supplied to
    `types.GeneratorType`, `collections.abc.Generator` or similar in the return-type annotation.

- `invalid-implicit-return` emits errors on synchronous generator functions where a non-`None` type
    is provided in the return-type annotation as the third type argument to `types.GeneratorType`,
    `collections.abc.Generator` or similar, yet the function body includes code paths that
    implicitly return `None`.

## Rule history

- v0.0.0-alpha.5: First added to ty as part of the pre-existing `invalid-return-type` diagnostic

- v0.0.102: `invalid-generator-return-type` was split out as a standalone rule from
    `invalid-return-type`, and `invalid-return-type` was renamed as `invalid-explicit-return`
