import asyncio


async def cleanup():
    try:
        await work()
    except asyncio.CancelledError:
        await finish()
    except BaseException:
        await finish()
    finally:
        async with manager:
            async for item in source:
                await finish()


class Manager:
    async def __aexit__(self, *args):
        await finish()
