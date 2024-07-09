import anyio
import asyncio
import trio


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
        async with trio.open_nursery() as nursery:
            ...


async def func():
    with anyio.move_on_after():
        ...


async def func():
    with anyio.fail_after():
        ...


async def func():
    with anyio.CancelScope():
        ...


async def func():
    with anyio.CancelScope():
        ...


async def func():
    with asyncio.timeout():
        ...


async def func():
    with asyncio.timeout_at():
        ...
