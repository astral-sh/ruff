async def gen():
    yield 1
    return 42

def gen(): # B901 but not a syntax error - not an async generator
    yield 1
    return 42

async def gen(): # ok - no value in return
    yield 1
    return

async def gen():
    yield 1
    return foo()

async def gen():
    yield 1
    return [1, 2, 3]

async def gen():
    if True:
        yield 1
    return 10
