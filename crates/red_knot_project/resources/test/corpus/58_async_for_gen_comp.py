async def foo():
    l = (i async for i in gen())
    return [i for i in l]

