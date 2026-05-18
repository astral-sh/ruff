"""
B019 should emit on every method below that is decorated with a method-caching
decorator (functools.lru_cache, functools.cache, async_lru.alru_cache, or one
of the aiocache decorators) AND is an instance method on a non-enum class.
"""
import functools
from functools import cache, cached_property, lru_cache

import async_lru
import aiocache
from async_lru import alru_cache
from aiocache import cached, cached_stampede, multi_cached


def some_other_cache():
    ...


@functools.cache
def compute_func(self, y):
    ...


class Foo:
    def __init__(self, x):
        self.x = x

    def compute_method(self, y):
        ...

    @some_other_cache
    def user_cached_instance_method(self, y):
        ...

    @classmethod
    @functools.cache
    def cached_classmethod(cls, y):
        ...

    @classmethod
    @cache
    def other_cached_classmethod(cls, y):
        ...

    @classmethod
    @functools.lru_cache
    def lru_cached_classmethod(cls, y):
        ...

    @classmethod
    @lru_cache
    def other_lru_cached_classmethod(cls, y):
        ...

    @staticmethod
    @functools.cache
    def cached_staticmethod(y):
        ...

    @staticmethod
    @cache
    def other_cached_staticmethod(y):
        ...

    @staticmethod
    @functools.lru_cache
    def lru_cached_staticmethod(y):
        ...

    @staticmethod
    @lru_cache
    def other_lru_cached_staticmethod(y):
        ...

    @functools.cached_property
    def some_cached_property(self):
        ...

    @cached_property
    def some_other_cached_property(self):
        ...

    # Remaining methods should emit B019
    @functools.cache
    def cached_instance_method(self, y):
        ...

    @cache
    def another_cached_instance_method(self, y):
        ...

    @functools.cache()
    def called_cached_instance_method(self, y):
        ...

    @cache()
    def another_called_cached_instance_method(self, y):
        ...

    @functools.lru_cache
    def lru_cached_instance_method(self, y):
        ...

    @lru_cache
    def another_lru_cached_instance_method(self, y):
        ...

    @functools.lru_cache()
    def called_lru_cached_instance_method(self, y):
        ...

    @lru_cache()
    def another_called_lru_cached_instance_method(self, y):
        ...


import enum


class Foo(enum.Enum):
    ONE = enum.auto()
    TWO = enum.auto()

    @functools.cache
    def bar(self, arg: str) -> str:
        return f"{self} - {arg}"


class Metaclass(type):
    @functools.lru_cache
    def lru_cached_instance_method_on_metaclass(cls, x: int):
        ...


# `async_lru.alru_cache` and `aiocache.{cached, cached_stampede, multi_cached}`
# have the same memory-leak failure mode as `functools.lru_cache` when applied
# to an instance method (they retain a strong reference to `self`).
class AsyncFoo:
    def __init__(self, x):
        self.x = x

    # Classmethods and staticmethods should be ignored, mirroring the synchronous case.
    @classmethod
    @async_lru.alru_cache
    async def alru_cached_classmethod(cls, y):
        ...

    @classmethod
    @alru_cache
    async def alru_cached_classmethod_imported(cls, y):
        ...

    @staticmethod
    @aiocache.cached()
    async def aiocache_cached_staticmethod(y):
        ...

    @staticmethod
    @cached()
    async def aiocache_cached_staticmethod_imported(y):
        ...

    # Remaining methods should emit B019.
    @async_lru.alru_cache
    async def alru_cached_instance_method(self, y):
        ...

    @alru_cache
    async def alru_cached_instance_method_imported(self, y):
        ...

    @async_lru.alru_cache(maxsize=128)
    async def called_alru_cached_instance_method(self, y):
        ...

    @aiocache.cached()
    async def aiocache_cached_instance_method(self, y):
        ...

    @cached()
    async def aiocache_cached_instance_method_imported(self, y):
        ...

    @aiocache.cached_stampede()
    async def aiocache_cached_stampede_instance_method(self, y):
        ...

    @cached_stampede()
    async def aiocache_cached_stampede_instance_method_imported(self, y):
        ...

    @aiocache.multi_cached()
    async def aiocache_multi_cached_instance_method(self, keys):
        ...

    @multi_cached()
    async def aiocache_multi_cached_instance_method_imported(self, keys):
        ...


class AsyncEnumFoo(enum.Enum):
    ONE = enum.auto()
    TWO = enum.auto()

    # Enum members are singletons, so caching their methods does not retain
    # any references that would not have been retained anyway.
    @async_lru.alru_cache
    async def alru_on_enum(self, arg: str) -> str:
        return f"{self} - {arg}"

    @aiocache.cached()
    async def aiocache_on_enum(self, arg: str) -> str:
        return f"{self} - {arg}"
