# NOASYNCIO
# NOANYIO - since anyio.Cancelled does not exist
import trio


async def foo(): ...


# except cancelled/baseexception are also critical
async def foo4():
    try:
        ...
    except trio.Cancelled:
        await foo()  # error: 8, Statement("trio.Cancelled", lineno-1)
    except:
        await foo()  # safe, since after trio.Cancelled

    try:
        ...
    except trio.Cancelled:
        await foo()  # error: 8, Statement("trio.Cancelled", lineno-1)
    except BaseException:
        await foo()  # safe, since after trio.Cancelled


async def foo5():
    try:
        ...
    except trio.Cancelled:
        with trio.CancelScope(deadline=30, shield=True):
            await foo()  # safe
    except BaseException:
        await foo()  # safe, since after trio.Cancelled


# trio.open_nursery is not a source of cancellations
async def foo_open_nursery_no_cancel():
    try:
        pass
    finally:
        # open_nursery does not block/checkpoint on entry, and is not
        # a cancellation point on exit.
        async with trio.open_nursery() as nursery:
            nursery.cancel_scope.deadline = trio.current_time() + 10
            nursery.cancel_scope.shield = True
            await foo()
