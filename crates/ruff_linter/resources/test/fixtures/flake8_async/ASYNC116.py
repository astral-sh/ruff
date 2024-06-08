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
