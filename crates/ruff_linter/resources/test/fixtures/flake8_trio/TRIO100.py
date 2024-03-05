import trio


async def func():
    with trio.fail_after():
        ...


async def func():
    with trio.fail_at():
        await ...


async def func():
    with trio.move_on_after():
        ...


async def func():
    with trio.move_at():
        await ...


async def func():
    with trio.move_at():
        async with trio.open_nursery() as nursery:
            ...
