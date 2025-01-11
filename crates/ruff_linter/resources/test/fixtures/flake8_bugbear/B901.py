"""
Should emit:
B901 - on lines 9, 17, 25
"""


def broken():
    if True:
        return [1, 2, 3]

    yield 3
    yield 2
    yield 1


def broken2():
    return [3, 2, 1]

    yield from not_broken()


def broken3(x=None):
    if x is None:
        return 0

    for i in range(3):
        (yield 0) if i > 1 else (yield 1)


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


async def not_broken4():
    import asyncio

    await asyncio.sleep(1)
    return 1


def not_broken5():
    def inner():
        return 2

    yield inner()


def not_broken6():
    return (yield from [])


def not_broken7():
    x = yield from []
    return x


def not_broken8():
    x = None

    def inner(ex):
        nonlocal x
        x = ex

    inner((yield from []))
    return x


class NotBroken9(object):
    def __await__(self):
        yield from function()
        return 42