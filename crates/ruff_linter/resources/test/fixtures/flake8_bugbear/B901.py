"""
Should emit:
B901 - on lines 9, 36, 56, 61, 72, 83, 87
"""


def broken():
    if True:
        return [1, 2, 3]

    yield 3
    yield 2
    yield 1


def not_broken():
    if True:
        return

    yield 3
    yield 2
    yield 1


def not_broken2():
    return not_broken()


def not_broken3():
    return

    yield from not_broken()


def broken2():
    return [3, 2, 1]

    yield from not_broken()


async def not_broken4():
    import asyncio

    await asyncio.sleep(1)
    return 1


def not_broken5():
    def inner():
        return 2

    yield inner()


def broken3():
    return (yield from [])


def broken4():
    x = yield from []
    return x


def not_broken6():
    x = None

    def inner(ex):
        nonlocal x
        x = ex

    inner((yield from []))
    return x


class NotBroken7(object):
    def __await__(self):
        yield from function()
        return 42

def broken5():
    x = yield
    print(x)
    return 42

def broken6():
    if True:
        return 42

    for i in range(10):
        (yield a for a in range(i))
