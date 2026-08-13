import trio
from abc import ABC, abstractmethod
from typing import override


async def func():
    ...


async def func(timeout):
    ...


async def func(timeout=10):
    ...


class Foo(ABC):
    @abstractmethod
    async def foo(self, timeout: float): ...


class Bar(Foo):
    @override
    async def foo(self, timeout: float): ...
