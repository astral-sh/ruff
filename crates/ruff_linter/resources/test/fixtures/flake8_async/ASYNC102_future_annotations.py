from __future__ import annotations

import trio


async def cleanup():
    try:
        pass
    finally:
        async def annotated(value: await factory()) -> await factory():
            await work()

        async def default(value=await factory()):  # ASYNC102
            await work()
