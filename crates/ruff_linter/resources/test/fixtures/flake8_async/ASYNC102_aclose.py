# type: ignore
# ARG --enable=ASYNC102,ASYNC120

# exclude finally: await x.aclose() from async102 and async120, with trio/anyio

# These magical markers are the ones that ensure trio & anyio don't raise errors:
# ANYIO_NO_ERROR
# TRIO_NO_ERROR

# See also async102_aclose_args.py - which makes sure trio/anyio raises errors if there
# are arguments to aclose().


async def foo():
    # no type tracking in this check, we allow any call that looks like
    # `await [...].aclose()`
    x = None

    try:
        ...
    except BaseException:
        await x.aclose()  # ASYNC102: 8, Statement("BaseException", lineno-1)
        await x.y.aclose()  # ASYNC102: 8, Statement("BaseException", lineno-2)
    finally:
        await x.aclose()  # ASYNC102: 8, Statement("try/finally", lineno-6)
        await x.y.aclose()  # ASYNC102: 8, Statement("try/finally", lineno-7)
