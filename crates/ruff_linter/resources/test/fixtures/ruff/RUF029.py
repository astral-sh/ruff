import time
import asyncio

async def pass_case():
    print("hello")
    await asyncio.sleep(1)
    print("world")

async def fail_case(): # RUF029
    print("hello")
    time.sleep(1)
    print("world")
