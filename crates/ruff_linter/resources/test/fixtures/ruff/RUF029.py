import time
import asyncio

async def pass_case(): # OK: awaits a coroutine
    print("hello")
    await asyncio.sleep(1)
    print("world")

async def fail_case(): # RUF029
    print("hello")
    time.sleep(1)
    print("world")


async def pass_case_2(): # OK: uses an async context manager
    async with None as i:
        pass

async def fail_case_2(): # RUF029
    with None as i:
        pass


async def pass_case_3(): # OK: uses an async loop
    async for i in []:
        pass

async def fail_case_3(): # RUF029
    for i in []:
        pass
