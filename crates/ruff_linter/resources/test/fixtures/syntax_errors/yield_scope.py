yield  # error
yield 1  # error
yield from 1  # error
await 1  # error
[(yield x) for x in range(3)]  # error


def f():
    yield  # okay
    yield 1  # okay
    yield from 1  # okay
    await 1  # okay


lambda: (yield)  # okay
lambda: (yield 1)  # okay
lambda: (yield from 1)  # okay
lambda: (await 1)  # okay


def outer():
    class C:
        yield 1  # error

    [(yield 1) for x in range(3)]  # error
    ((yield 1) for x in range(3))  # error
    {(yield 1) for x in range(3)}  # error
    {(yield 1): 0 for x in range(3)}  # error
    {0: (yield 1) for x in range(3)}  # error


async def outer():
    [await x for x in range(3)]  # okay, comprehensions don't break async scope

    class C:
        [await x for x in range(3)]  # error, classes break async scope

    lambda x: await x  # okay for now, lambda breaks _async_ scope but is a function


await 1  # error
