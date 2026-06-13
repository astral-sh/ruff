## What it does

Checks for `await` being used with types that are not [Awaitable][awaitable-abc].

## Why is this bad?

Such expressions will lead to `TypeError` being raised at runtime.

## Examples

```python
import asyncio


class InvalidAwait:
    def __await__(self) -> int:
        return 5


async def main() -> None:
    await InvalidAwait()  # error: [invalid-await]
    await 42  # error: [invalid-await]


asyncio.run(main())
```

[awaitable-abc]: https://docs.python.org/3/library/collections.abc.html#collections.abc.Awaitable
