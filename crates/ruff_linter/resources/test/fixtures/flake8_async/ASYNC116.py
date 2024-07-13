# type: ignore
# ASYNCIO_NO_ERROR - no asyncio.sleep_forever, so check intentionally doesn't trigger.
import math
from math import inf


async def import_trio():
    import trio

    # These examples are probably not meant to ever wake up:
    await trio.sleep(100000)  # error: 116, "async"

    # 'inf literal' overflow trick
    await trio.sleep(1e999)  # error: 116, "async"

    await trio.sleep(86399)
    await trio.sleep(86400)
    await trio.sleep(86400.01)  # error: 116, "async"
    await trio.sleep(86401)  # error: 116, "async"

    await trio.sleep(-1)  # will raise a runtime error
    await trio.sleep(0)  # handled by different check

    # these ones _definitely_ never wake up (TODO)
    await trio.sleep(float("inf"))
    await trio.sleep(math.inf)
    await trio.sleep(inf)

    # don't require inf to be in math (TODO)
    await trio.sleep(np.inf)

    # don't evaluate expressions (TODO)
    one_day = 86401
    await trio.sleep(86400 + 1)
    await trio.sleep(60 * 60 * 24 + 1)
    await trio.sleep(foo())
    await trio.sleep(one_day)
    await trio.sleep(86400 + foo())
    await trio.sleep(86400 + ...)
    await trio.sleep("hello")
    await trio.sleep(...)


def not_async_fun():
    import trio

    # does not require the call to be awaited, nor in an async fun
    trio.sleep(86401)  # error: 116, "async"
    # also checks that we don't break visit_Call
    trio.run(trio.sleep(86401))  # error: 116, "async"


async def import_from_trio():
    from trio import sleep

    # catch from import
    await sleep(86401)  # error: 116, "async"


async def import_anyio():
    import anyio

    # These examples are probably not meant to ever wake up:
    await anyio.sleep(100000)  # error: 116, "async"

    # 'inf literal' overflow trick
    await anyio.sleep(1e999)  # error: 116, "async"

    await anyio.sleep(86399)
    await anyio.sleep(86400)
    await anyio.sleep(86400.01)  # error: 116, "async"
    await anyio.sleep(86401)  # error: 116, "async"

    await anyio.sleep(-1)  # will raise a runtime error
    await anyio.sleep(0)  # handled by different check

    # these ones _definitely_ never wake up (TODO)
    await anyio.sleep(float("inf"))
    await anyio.sleep(math.inf)
    await anyio.sleep(inf)

    # don't require inf to be in math (TODO)
    await anyio.sleep(np.inf)

    # don't evaluate expressions (TODO)
    one_day = 86401
    await anyio.sleep(86400 + 1)
    await anyio.sleep(60 * 60 * 24 + 1)
    await anyio.sleep(foo())
    await anyio.sleep(one_day)
    await anyio.sleep(86400 + foo())
    await anyio.sleep(86400 + ...)
    await anyio.sleep("hello")
    await anyio.sleep(...)


def not_async_fun():
    import anyio

    # does not require the call to be awaited, nor in an async fun
    anyio.sleep(86401)  # error: 116, "async"
    # also checks that we don't break visit_Call
    anyio.run(anyio.sleep(86401))  # error: 116, "async"


async def import_from_anyio():
    from anyio import sleep

    # catch from import
    await sleep(86401)  # error: 116, "async"
