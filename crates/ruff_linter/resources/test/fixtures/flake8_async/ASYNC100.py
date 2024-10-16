import anyio
import asyncio
import trio
from contextlib import nullcontext


async def func():
    with trio.fail_after():
        ...


async def func():
    with trio.fail_at():
        await ...


async def func():
    with trio.move_on_after():
        ...


async def func():
    with trio.move_at():
        await ...


async def func():
    with trio.move_at():
        async with trio.open_nursery():
            ...


async def func():
    with trio.move_at():
        async for x in ...:
            ...


async def main():
    async with asyncio.timeout(7):
        print({i async for i in long_running_range()})


async def func():
    with anyio.move_on_after(delay=0.2):
        ...


async def func():
    with anyio.fail_after():
        ...


async def func():
    with anyio.CancelScope():
        ...


async def func():
    with anyio.CancelScope(), nullcontext():
        ...


async def func():
    with nullcontext(), anyio.CancelScope():
        ...


async def func():
    async with asyncio.timeout(delay=0.2):
        ...


async def func():
    async with asyncio.timeout_at(when=0.2):
        ...


async def func():
    async with asyncio.timeout(delay=0.2), asyncio.TaskGroup():
        ...


async def func():
    async with asyncio.timeout(delay=0.2), asyncio.TaskGroup(), asyncio.timeout(delay=0.2):
        ...


async def func():
    async with asyncio.timeout(delay=0.2), asyncio.TaskGroup(), asyncio.timeout(delay=0.2), asyncio.TaskGroup():
        ...


async def func():
    async with asyncio.timeout(delay=0.2), asyncio.timeout(delay=0.2):
        ...


# Don't trigger for blocks with a yield statement
async def foo():
    with trio.fail_after(1):
        yield


async def foo(): # even if only one branch contains a yield, we skip the lint
    with trio.fail_after(1):
        if something:
            ...
        else:
            yield


# https://github.com/astral-sh/ruff/issues/12873
@asynccontextmanager
async def good_code():
    with anyio.fail_after(10):
        # There's no await keyword here, but we presume that there
        # will be in the caller we yield to, so this is safe.
        yield
