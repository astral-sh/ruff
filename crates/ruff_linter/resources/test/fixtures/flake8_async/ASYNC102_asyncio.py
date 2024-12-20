# type: ignore
# NOANYIO
# NOTRIO
# BASE_LIBRARY asyncio
from contextlib import asynccontextmanager

import asyncio


async def foo():
    # asyncio.move_on_after does not exist, so this will raise an error
    try:
        ...
    finally:
        with asyncio.move_on_after(deadline=30) as s:
            s.shield = True
            await foo()  # error: 12, Statement("try/finally", lineno-5)

    try:
        pass
    finally:
        await foo()  # error: 8, Statement("try/finally", lineno-3)

    # asyncio.CancelScope does not exist, so this will raise an error
    try:
        pass
    finally:
        with asyncio.CancelScope(deadline=30, shield=True):
            await foo()  # error: 12, Statement("try/finally", lineno-4)

    # TODO: I think this is the asyncio-equivalent, but functionality to ignore the error
    # has not been implemented

    try:
        ...
    finally:
        await asyncio.shield(  # error: 8, Statement("try/finally", lineno-3)
            asyncio.wait_for(foo())
        )


# asyncio.TaskGroup *is* a source of cancellations (on exit)
async def foo_open_nursery_no_cancel():
    try:
        pass
    finally:
        async with asyncio.TaskGroup() as tg:  # error: 8, Statement("try/finally", lineno-3)
            ...
