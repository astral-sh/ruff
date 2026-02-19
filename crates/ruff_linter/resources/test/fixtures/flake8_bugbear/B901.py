"""
Should emit:
B901 - on lines 9, 36
"""
from collections.abc import Generator

from pluggy import hookimpl as pluggy_hookimpl
from pytest import TestReport, hookimpl


def broken():
    if True:
        return [1, 2, 3]

    yield 3
    yield 2
    yield 1


def not_broken():
    if True:
        return

    yield 3
    yield 2
    yield 1


def not_broken2():
    return not_broken()


def not_broken3():
    return

    yield from not_broken()


def broken2():
    return [3, 2, 1]

    yield from not_broken()


async def not_broken4():
    import asyncio

    await asyncio.sleep(1)
    return 1


def not_broken5():
    def inner():
        return 2

    yield inner()


def broken3():
    return (yield from [])


def broken4():
    x = yield from []
    return x


def broken5():
    x = None

    def inner(ex):
        nonlocal x
        x = ex

    inner((yield from []))
    return x


class NotBroken9(object):
    def __await__(self):
        yield from function()
        return 42


async def broken6():
    yield 1
    return foo()


async def broken7():
    yield 1
    return [1, 2, 3]


@hookimpl(wrapper=True)
def pytest_runtest_makereport() -> Generator[None, TestReport, TestReport]:
    result = yield
    result.outcome = "passed"
    return result


@pluggy_hookimpl(wrapper=True)
def pluggy_hook() -> Generator[None, int, int]:
    result = yield
    return result + 1


@hookimpl(wrapper=False)
def broken_wrapper_false() -> Generator[None, int, int]:
    result = yield
    return result


@hookimpl
def broken_no_wrapper() -> Generator[None, int, int]:
    result = yield
    return result
