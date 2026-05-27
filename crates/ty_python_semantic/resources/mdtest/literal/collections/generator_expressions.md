# Generator expressions

## Basic

We infer specialized `GeneratorType` instance types for generator expressions:

```py
# revealed: GeneratorType[int, None, None]
reveal_type(x for x in range(10))

# revealed: GeneratorType[tuple[int, str], None, None]
reveal_type((x, str(y)) for x in range(3) for y in range(3))
```

## PEP 798 unpacking generator expressions

```toml
[environment]
python-version = "3.15"
```

```py
list_of_lists: list[list[int]] = [[1], [2, 3]]
not_iterables: list[int] = [1, 2]

# revealed: GeneratorType[int, None, None]
reveal_type(*xs for xs in list_of_lists)

(*value for value in not_iterables)  # error: [not-iterable] "Object of type `int` is not iterable"
```

When used in a loop, the yielded type can be inferred:

```py
squares = (x**2 for x in range(10))

for s in squares:
    reveal_type(s)  # revealed: int
```

`GeneratorType` is covariant in its yielded type, so it can be used where a wider yielded type is
expected:

```py
from typing import Iterator

def process_numbers(x: Iterator[float]): ...

numbers = (x for x in range(10))
reveal_type(numbers)  # revealed: GeneratorType[int, None, None]
process_numbers(numbers)
```

## Async generators

For async generator expressions, we infer specialized `AsyncGeneratorType` instance types:

```py
import asyncio
from typing import AsyncGenerator

async def slow_numbers() -> AsyncGenerator[int, None]:
    current = 0
    while True:
        await asyncio.sleep(1)
        yield current
        current += 1

async def main() -> None:
    slow_squares = (x**2 async for x in slow_numbers())

    reveal_type(slow_squares)  # revealed: AsyncGeneratorType[int, None]

    async for s in slow_squares:
        reveal_type(s)  # revealed: int
        print(s)

asyncio.run(main())
```
