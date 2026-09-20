"""Test for ASYNC102/ASYNC120 with except*

ASYNC102: await-in-finally-or-cancelled

ASYNC120: await-in-except
"""

# type: ignore
# ARG --enable=ASYNC102,ASYNC120
# NOASYNCIO # TODO: support asyncio shields
import trio


async def foo():
    try:
        ...
    except* ValueError:
        await foo()  # ASYNC120: 8, Statement("except", lineno-1)
        raise
    except* BaseException:
        await foo()  # ASYNC102: 8, Statement("BaseException", lineno-1)
    finally:
        await foo()  # ASYNC102: 8, Statement("try/finally", lineno-8)

    try:
        ...
    except* BaseException:
        with trio.move_on_after(30, shield=True):
            await foo()
