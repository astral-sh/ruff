# pylint: disable=missing-docstring,unused-variable
import asyncio


async def nested():
    return 42


async def main():
    nested()
    print(await nested())  # This is okay


def not_async():
    print(await nested())  # [await-outside-async]


async def func(i):
    return i**2


async def okay_function():
    var = [await func(i) for i in range(5)]  # This should be okay


# Test nested functions
async def func2():
    def inner_func():
        await asyncio.sleep(1)  # [await-outside-async]


def outer_func():
    async def inner_func():
        await asyncio.sleep(1)


def async_for_loop():
    async for x in foo():
        pass


def async_with():
    async with foo():
        pass


# See: https://github.com/astral-sh/ruff/issues/14167
def async_for_generator_elt():
    (await x for x in foo())


# See: https://github.com/astral-sh/ruff/issues/14167
def async_for_list_comprehension_elt():
    [await x for x in foo()]


# See: https://github.com/astral-sh/ruff/issues/14167
def async_for_list_comprehension():
    [x async for x in foo()]


# See: https://github.com/astral-sh/ruff/issues/14167
def await_generator_iter():
    (x for x in await foo())


# See: https://github.com/astral-sh/ruff/issues/14167
def await_generator_target():
    (x async for x in foo())


# See: https://github.com/astral-sh/ruff/issues/14167
def async_for_list_comprehension_target():
    [x for x in await foo()]


def async_for_dictionary_comprehension_key():
    {await x: y for x, y in foo()}


def async_for_dictionary_comprehension_value():
    {y: await x for x, y in foo()}


def async_for_dict_comprehension():
    {x: y async for x, y in foo()}
