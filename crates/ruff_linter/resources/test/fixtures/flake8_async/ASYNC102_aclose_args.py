# type: ignore

# trio/anyio should still raise errors if there's args
# asyncio will always raise errors

# See also async102_aclose.py, which checks that trio/anyio marks arg-less aclose() as safe


async def foo():
    # no type tracking in this check
    x = None

    try:
        ...
    except BaseException:
        await x.aclose(foo)  # ASYNC102: 8, Statement("BaseException", lineno-1)
        await x.aclose(bar=foo)  # ASYNC102: 8, Statement("BaseException", lineno-2)
        await x.aclose(*foo)  # ASYNC102: 8, Statement("BaseException", lineno-3)
        await x.aclose(None)  # ASYNC102: 8, Statement("BaseException", lineno-4)
    finally:
        await x.aclose(foo)  # ASYNC102: 8, Statement("try/finally", lineno-8)
        await x.aclose(bar=foo)  # ASYNC102: 8, Statement("try/finally", lineno-9)
        await x.aclose(*foo)  # ASYNC102: 8, Statement("try/finally", lineno-10)
        await x.aclose(None)  # ASYNC102: 8, Statement("try/finally", lineno-11)
