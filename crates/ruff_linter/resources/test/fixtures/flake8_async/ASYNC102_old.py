import asyncio

import anyio
from anyio import CancelScope
from anyio import CancelScope as CS

async def pass_no_await():
    try:
        pass
    except Exception as e:
        pass


async def pass_shielded():
    try:
        pass
    except Exception as e:
        with anyio.CancelScope(shield=True):
            await asyncio.sleep(1)
        with CancelScope(shield=True):
            await asyncio.sleep(1)
        with CS(shield=True):
            await asyncio.sleep(1)


async def pass_shielded_in_multiple_ways():
    try:
        pass
    except Exception as e:
        with anyio.CancelScope(shield=True):
            await asyncio.sleep(1)
        with anyio.move_on_after(shield=True):
            await asyncio.sleep(1)
        with anyio.fail_after(shield=True):
            await asyncio.sleep(1)


async def fail_context_manager_but_not_shielded_in_multiple_ways():
    try:
        pass
    except Exception as e:
        with anyio.CancelScope(shield=False):
            await asyncio.sleep(1)  # fail
        with anyio.CancelScope():
            await asyncio.sleep(1)  # fail
        with anyio.move_on_after(shield=False):
            await asyncio.sleep(1)  # fail
        with anyio.move_on_after():
            await asyncio.sleep(1)  # fail
        with anyio.fail_after(shield=False):
            await asyncio.sleep(1)  # fail
        with anyio.fail_after():
            await asyncio.sleep(1)  # fail


async def pass_just_value_error():
    try:
        pass
    except ValueError as e:
        await asyncio.sleep(1)


async def fail_exception():
    try:
        pass
    except Exception as e:
        await asyncio.sleep(1)  # fail


async def fail_cancelled_error():
    try:
        pass
    except asyncio.CancelledError as e:
        await asyncio.sleep(1)  # fail


async def fail_nested_exception():
    try:
        pass
    except ((ZeroDivisionError, Exception), ValueError) as e:
        await asyncio.sleep(1)  # fail


async def fail_finally():
    try:
        pass
    finally:
        await asyncio.sleep(1)  # fail

async def fail_async_with():
    try:
        pass
    finally:
        async with my_own_async_manager():  # fail
            pass
