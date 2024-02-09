import asyncio

# Violation cases: RUF066


async def test_coroutine_without_await():
    async def coro():
        pass

    coro()  # RUF066


async def test_coroutine_without_await():
    async def coro():
        pass

    result = coro()  # RUF066


async def test_coroutine_without_await():
    def not_coro():
        pass

    async def coro():
        pass

    not_coro()
    coro()  # RUF066


async def test_coroutine_without_await():
    async def coro():
        another_coro()  # RUF066

    async def another_coro():
        pass

    await coro()


async def test_asyncio_api_without_await():
    asyncio.sleep(0.5)  # RUF066


async def test_asyncio_api_without_await():
    async def coro():
        asyncio.sleep(0.5)  # RUF066

    await asyncio.wait(coro)


async def test_asyncio_api_without_await():
    async def coro():
        await asyncio.sleep(0.5)

    asyncio.wait_for(coro)  # RUF066


async def test_asyncio_api_without_await():
    async def coro1():
        await asyncio.sleep(0.5)

    async def coro2():
        await asyncio.sleep(0.5)

    tasks = [coro1(), coro2()]
    asyncio.gather(*tasks)  # RUF066


# Non-violation cases: RUF066


async def test_coroutine_with_await():
    async def coro():
        pass

    await coro()  # OK


async def test_coroutine_with_await():
    def not_coro():
        pass

    async def coro():
        pass

    not_coro()
    await coro()  # OK


import asyncio


# define an asynchronous context manager
class AsyncContextManager:
    # enter the async context manager
    async def __aenter__(self):
        await asyncio.sleep(0.5)

    async def __aexit__(self, exc_type, exc, tb):
        await asyncio.sleep(0.5)


# define a simple coroutine
async def custom_coroutine():
    # create and use the asynchronous context manager
    async with AsyncContextManager():  # OK
        ...


async def test_coroutine_in_func_arg():
    async def another_coro():
        pass

    async def coro(cr):
        await cr

    await coro(another_coro())  # OK


async def test_coroutine_with_yield():
    async def another_coro():
        pass

    async def coro():
        yield another_coro()

    await coro()  # OK


async def test_coroutine_with_return():
    async def another_coro():
        pass

    async def coro():
        return another_coro()

    await coro()  # OK


async def test_coroutine_with_async_iterator():
    class Counter:
        def __init__(self):
            pass

        def __aiter__(self):
            return self

        async def __anext__(self):
            pass

    async def main():
        async for c in Counter():  # OK
            pass


async def test_asyncio_api_with_await():
    async def task_coro(value):
        await asyncio.sleep(1)
        return value * 10

    # main coroutine
    async def main():
        awaitables = [task_coro(i) for i in range(10)]
        await asyncio.gather(*awaitables)  # OK


async def test_coroutine_inside_collections():
    async def coro():
        pass

    [coro(), coro()]  # OK
    (coro(), coro())  # OK
    {coro(), coro()}  # OK
    {"coro": coro()}  # OK


async def test_func_used_in_arg_should_not_raise(func):
    func()  # OK
