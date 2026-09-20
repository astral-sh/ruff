# Without evidence of Trio or AnyIO, Ruff does not assume level cancellation.
async def cleanup():
    try:
        await work()
    except BaseException:
        await finish()
    finally:
        await finish()


async def __aexit__(*args):
    await finish()
