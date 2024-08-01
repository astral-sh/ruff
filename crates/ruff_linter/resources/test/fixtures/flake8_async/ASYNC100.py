import anyio
import asyncio
import trio


async def func():
    async with trio.fail_after():
        ...


async def func():
    async with trio.fail_at():
        await ...


async def func():
    async with trio.move_on_after():
        ...


async def func():
    async with trio.move_at():
        await ...


async def func():
    async with trio.move_at():
        async with trio.open_nursery():
            ...


async def func():
    async with anyio.move_on_after(delay=0.2):
        ...


async def func():
    async with anyio.fail_after():
        ...


async def func():
    async with anyio.CancelScope():
        ...


async def func():
    async with anyio.CancelScope():
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
