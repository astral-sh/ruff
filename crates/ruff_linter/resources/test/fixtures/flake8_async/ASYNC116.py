# type: ignore
# ASYNCIO_NO_ERROR - no asyncio.sleep_forever, so check intentionally doesn't trigger.
import math
from math import inf

import trio


async def foo():
    # These examples are probably not meant to ever wake up:
    await trio.sleep(100000)  # error: 10, "trio"

    # and these ones _definitely_ never wake up:
    await trio.sleep(float("inf"))  # error: 10, "trio"
    await trio.sleep(math.inf)  # error: 10, "trio"
    await trio.sleep(inf)  # error: 10, "trio"

    # 'inf literal' overflow trick
    await trio.sleep(1e999)  # error: 10, "trio"

    await trio.sleep(86399)
    await trio.sleep(86400)
    await trio.sleep(86400.01)  # error: 10, "trio"
    await trio.sleep(86401)  # error: 10, "trio"

    await trio.sleep(-1)  # will raise a runtime error from Trio
    await trio.sleep(0)  # handled by different check

    # don't evaluate expressions
    await trio.sleep(10**10)
    await trio.sleep(86400 + 1)
    await trio.sleep(foo())
    await trio.sleep(86400 + foo())  # type: ignore
    await trio.sleep(86400 + ...)  # type: ignore
    await trio.sleep("hello")
    await trio.sleep(...)

    # don't require inf to be in math #103
    await trio.sleep(np.inf)  # error: 10, "trio"
    await trio.sleep(anything.inf)  # error: 10, "trio"
    await trio.sleep(anything.anything.inf)  # error: 10, "trio"
    await trio.sleep(inf.inf)  # error: 10, "trio"
    await trio.sleep(inf.anything)


# does not require the call to be awaited, nor in an async fun
trio.sleep(86401)  # error: 0, "trio"
# also checks that we don't break visit_Call
trio.run(trio.sleep(86401))  # error: 9, "trio"