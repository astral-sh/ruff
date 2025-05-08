async def elements(n):
    yield n

def regular_function():
    [x async for x in elements(1)]

    async with elements(1) as x:
        pass

    async for _ in elements(1):
        pass
