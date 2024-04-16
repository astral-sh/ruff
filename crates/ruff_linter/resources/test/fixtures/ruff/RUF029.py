import time
import asyncio


async def pass_case():  # OK: awaits a coroutine
    print("hello")
    await asyncio.sleep(1)
    print("world")


async def fail_case():  # RUF029
    print("hello")
    time.sleep(1)
    print("world")


async def pass_case_2():  # OK: uses an async context manager
    async with None as i:
        pass


async def fail_case_2():  # RUF029
    with None as i:
        pass


async def pass_case_3():  # OK: uses an async loop
    async for i in []:
        pass


async def fail_case_3():  # RUF029
    for i in []:
        pass


async def pass_case_4():  # OK: awaits a coroutine
    def foo(optional_arg=await bar()):
        pass

    return foo


async def fail_case_5():  # RUF029: yield does not require async
    yield "hello"


async def fail_case_6():  # RUF029: the /outer/ function does not await
    async def foo():
        await bla


class Foo:
    async def pass_case_7():  # OK: method of a class
        pass


async def fail_case_7():  # RUF029: the /outer/ function does not await
    class Foo:
        async def foo():
            await bla
