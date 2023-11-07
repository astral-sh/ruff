import trio

async def foo():
    while True:
        await trio.sleep(10)

async def foo():
    while True:
        await trio.sleep_until(10)

async def foo():
    while True:
        trio.sleep(10)
