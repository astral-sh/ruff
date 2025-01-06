def func1(str, /, type, *complex, Exception, **getattr):
    pass


async def func2(bytes):
    pass


async def func3(id, dir):
    pass


# this is Ok for A002 (trigger A005 instead)
# https://github.com/astral-sh/ruff/issues/14135
map([], lambda float: ...)

from typing import override, overload


@override
def func4(id, dir):
    pass


@overload
def func4(id, dir):
    pass
