import trio


async def foo():
    with trio.fail_after():
        ...

async def foo():
    with trio.fail_at():
        await ...

async def foo():
    with trio.move_on_after():
        ...

async def foo():
    with trio.move_at():
        await ...
