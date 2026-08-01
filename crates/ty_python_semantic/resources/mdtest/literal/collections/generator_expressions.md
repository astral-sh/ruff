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

An `await` expression in the generator expression's implicit scope also makes it asynchronous:

```py
async def is_even(value: int) -> bool:
    return value % 2 == 0

async def filter_values() -> None:
    values: list[int] = [1, 2, 3]
    generator = (value for value in values if await is_even(value))
    reveal_type(generator)  # revealed: AsyncGeneratorType[int, None]
    await anext(generator, None)
```

This includes `await` expressions in the yielded element and in the iterable of any `for` clause
after the first:

```py
async def get_values(value: int = 0) -> list[int]:
    return [value]

async def get_value(value: int) -> int:
    return value

async def await_locations() -> None:
    reveal_type([await get_value(x)] for x in [1])  # revealed: AsyncGeneratorType[list[int], None]
    reveal_type(y for x in [1] for y in await get_values(x))  # revealed: AsyncGeneratorType[int, None]
```

The iterable in the first `for` clause is evaluated in the enclosing scope, so an `await` there does
not make the generator expression asynchronous. The same rule means that the first iterable of a
nested generator expression can make the outer generator asynchronous:

```py
async def first_iterable() -> None:
    reveal_type(x for x in await get_values())  # revealed: GeneratorType[int, None, None]
    outer = ((y for y in await get_values()) for _ in [1])
    reveal_type(outer)  # revealed: AsyncGeneratorType[GeneratorType[int, None, None], None]
```

An asynchronous nested generator expression does not make the outer generator asynchronous, but an
asynchronous nested container comprehension does:

```py
from collections.abc import AsyncIterator

async def async_values() -> AsyncIterator[int]:
    yield 1

async def nested_comprehensions() -> None:
    nested_generator = ((await get_value(x) for x in [1]) for _ in [1])
    reveal_type(nested_generator)  # revealed: GeneratorType[AsyncGeneratorType[int, None], None, None]
    reveal_type([x async for x in async_values()] for _ in [1])  # revealed: AsyncGeneratorType[list[int], None]
    reveal_type({x async for x in async_values()} for _ in [1])  # revealed: AsyncGeneratorType[set[int], None]
    reveal_type({x: x async for x in async_values()} for _ in [1])  # revealed: AsyncGeneratorType[dict[int, int], None]
```
