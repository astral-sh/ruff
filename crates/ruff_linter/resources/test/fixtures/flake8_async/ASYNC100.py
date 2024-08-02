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
