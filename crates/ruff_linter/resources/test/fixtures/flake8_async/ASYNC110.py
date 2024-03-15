import trio


async def func():
    while True:
        await trio.sleep(10)


async def func():
    while True:
        await trio.sleep_until(10)


async def func():
    while True:
        trio.sleep(10)
