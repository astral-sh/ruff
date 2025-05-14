# Async with statements

## Basic `async with` statement

The type of the target variable in a `with` statement should be the return type from the context
manager's `__aenter__` method. However, `async with` statements aren't supported yet. This test
asserts that it doesn't emit any context manager-related errors.

```py
class Target: ...

class Manager:
    async def __aenter__(self) -> Target:
        return Target()

    async def __aexit__(self, exc_type, exc_value, traceback): ...

async def test():
    async with Manager() as f:
        reveal_type(f)  # revealed: @Todo(async `with` statement)
```
