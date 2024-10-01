import trio
import anyio
import asyncio


async def func():
    while True:
        await trio.sleep(10)


async def func():
    while True:
        await trio.sleep_until(10)


async def func():
    while True:
        trio.sleep(10)


async def func():
    while True:
        await anyio.sleep(10)


async def func():
    while True:
        await anyio.sleep_until(10)


async def func():
    while True:
        anyio.sleep(10)


async def func():
    while True:
        await asyncio.sleep(10)


async def func():
    while True:
        await asyncio.sleep_until(10)


async def func():
    while True:
        asyncio.sleep(10)
