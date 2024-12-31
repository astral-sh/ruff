"""
Should emit:
B901 - on lines 9, 17, 25, 30, 35, 42, 48, 53
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


def broken3():
    x = yield
    print(x)
    return 42


def broken4():
    (yield from range(5))
    return 10


def broken5():
    x, y = ((yield from []), 7)
    return y


def broken6():
    x = y = z = yield from []
    w, z = ("a", 10)
    x
    return z


def broken7():
    x = yield from []
    x = 5
    return x


def broken8():
    ((x, y), z) = ((a, b), c) = (((yield 2), 3), 4)
    return b


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


def not_broken9():
    x = None

    def inner():
        return (yield from [])

    x = inner()
    return x


def not_broken10():
    x, y = ((yield from []), 7)
    return x


def not_broken11():
    x = y = z = yield from []
    return z


def not_broken12():
    x = yield
    print(x)
    return x


def not_broken13():
    (x, y), z, w = ((0, (yield)), 1, 2)
    return y


def not_broken14():
    (x, y) = (z, w) = ((yield 5), 7)
    return z


class NotBroken9(object):
    def __await__(self):
        yield from function()
        return 42