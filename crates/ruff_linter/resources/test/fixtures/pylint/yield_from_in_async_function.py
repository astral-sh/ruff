async def success():
    yield 42


async def fail():
    l = (1, 2, 3)
    yield from l
