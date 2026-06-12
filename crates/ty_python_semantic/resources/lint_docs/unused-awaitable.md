## What it does

Checks for awaitable objects (such as coroutines) used as expression
statements without being awaited.

## Why is this bad?

Calling an `async def` function returns a coroutine object. If the
coroutine is never awaited, the body of the async function will never
execute, which is almost always a bug. Python emits a
`RuntimeWarning: coroutine was never awaited` at runtime in this case.

## Examples

```python
async def fetch_data() -> str:
    return "data"


async def main() -> None:
    fetch_data()  # Warning: coroutine is not awaited
    await fetch_data()  # OK
```
