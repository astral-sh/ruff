import time
import asyncio


async def func():
    time.sleep(1)  # ASYNC251


def func():
    time.sleep(1)  # OK


async def func():
    asyncio.sleep(1)  # OK
