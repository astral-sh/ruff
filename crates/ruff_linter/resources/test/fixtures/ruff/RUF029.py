import time
import asyncio


async def pass_1a():  # OK: awaits a coroutine
    await asyncio.sleep(1)


async def pass_1b():  # OK: awaits a coroutine
    def foo(optional_arg=await bar()):
        pass


async def pass_2():  # OK: uses an async context manager
    async with None as i:
        pass


async def pass_3():  # OK: uses an async loop
    async for i in []:
        pass


class Foo:
    async def pass_4(self):  # OK: method of a class
        pass


def foo():
    async def pass_5():  # OK: uses an await
        await bla


async def pass_6():  # OK: just a stub
    ...


async def fail_1a():  # RUF029
    time.sleep(1)


async def fail_1b():  # RUF029: yield does not require async
    yield "hello"


async def fail_2():  # RUF029
    with None as i:
        pass


async def fail_3():  # RUF029
    for i in []:
        pass

    return foo


async def fail_4a():  # RUF029: the /outer/ function does not await
    async def foo():
        await bla


async def fail_4b():  # RUF029: the /outer/ function does not await
    class Foo:
        async def foo(self):
            await bla


def foo():
    async def fail_4c():  # RUF029: the /inner/ function does not await
        pass


async def test():
    return [check async for check in async_func()]


async def test() -> str:
    vals = [str(val) for val in await async_func(1)]
    return ",".join(vals)
