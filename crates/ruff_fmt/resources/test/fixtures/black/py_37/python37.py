#!/usr/bin/env python3.7


def f():
    return (i * 2 async for i in arange(42))


def g():
    return (
        something_long * something_long
        async for something_long in async_generator(with_an_argument)
    )


async def func():
    if test:
        out_batched = [
            i
            async for i in aitertools._async_map(
                self.async_inc, arange(8), batch_size=3
            )
        ]


def awaited_generator_value(n):
    return (await awaitable for awaitable in awaitable_list)


def make_arange(n):
    return (i * 2 for i in range(n) if await wrap(i))


# output


#!/usr/bin/env python3.7


def f():
    return (i * 2 async for i in arange(42))


def g():
    return (
        something_long * something_long
        async for something_long in async_generator(with_an_argument)
    )


async def func():
    if test:
        out_batched = [
            i
            async for i in aitertools._async_map(
                self.async_inc, arange(8), batch_size=3
            )
        ]


def awaited_generator_value(n):
    return (await awaitable for awaitable in awaitable_list)


def make_arange(n):
    return (i * 2 for i in range(n) if await wrap(i))
