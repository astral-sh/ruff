import aiocache
from aiocache import cached, cached_stampede, multi_cached
import async_lru
from async_lru import alru_cache
import functools
from functools import cache, cached_property, lru_cache


def some_other_cache():
    ...


@functools.cache
def compute_func(self, y):
    ...

@async_lru.alru_cache
async def async_lru_compute_func(self, y):
    ...

@aiocache.cached
async def aiocache_compute_func(self, y):
    ...

@aiocache.cached_stampede
async def aiocache_compute_func(self, y):
    ...

@aiocache.multi_cached
async def aiocache_compute_func(self, y):
    ...

class Foo:
    def __init__(self, x):
        self.x = x

    def compute_method(self, y):
        ...

    @some_other_cache
    def user_cached_instance_method(self, y):
        ...
    
    @some_other_cache
    async def async_user_cached_instance_method(self, y):
        ...

    # functools

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

    # async_lru

    @classmethod
    @async_lru.alru_cache
    def alru_cached_classmethod(cls, y):
        ...

    @classmethod
    @alru_cache
    def other_alru_cached_classmethod(cls, y):
        ...

    @staticmethod
    @async_lru.alru_cache
    def alru_cached_staticmethod(y):
        ...

    @staticmethod
    @alru_cache
    def other_alru_cached_staticmethod(y):
        ...
    
    # aiocache

    @classmethod
    @aiocache.cached
    def aiocache_cached_classmethod(cls, y):
        ...

    @classmethod
    @cached
    def other_aiocache_cached_classmethod(cls, y):
        ...

    @staticmethod
    @aiocache.cached
    def aiocache_cached_staticmethod(y):
        ...

    @staticmethod
    @cached
    def other_aiocache_cached_staticmethod(y):
        ...

    @classmethod
    @aiocache.cached_stampede
    def aiocache_cached_stampede_classmethod(cls, y):
        ...

    @classmethod
    @cached_stampede
    def other_aiocache_cached_stampede_classmethod(cls, y):
        ...

    @staticmethod
    @aiocache.cached_stampede
    def aiocache_cached_stampede_staticmethod(y):
        ...

    @staticmethod
    @cached_stampede
    def other_aiocache_cached_stampede_staticmethod(y):
        ...

    @classmethod
    @aiocache.multi_cached
    def aiocache_multi_cached_classmethod(cls, y):
        ...

    @classmethod
    @multi_cached
    def other_aiocache_multi_cached_classmethod(cls, y):
        ...

    @staticmethod
    @aiocache.multi_cached
    def aiocache_multi_cached_staticmethod(y):
        ...

    @staticmethod
    @multi_cached
    def other_aiocache_multi_cached_staticmethod(y):
        ...

    # Remaining methods should emit B019

    # functools
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
    
    # async_lru

    @async_lru.alru_cache
    async def alru_cached_instance_method(self, y):
        ...

    @alru_cache
    async def another_alru_cached_instance_method(self, y):
        ...

    @async_lru.alru_cache()
    async def called_alru_cached_instance_method(self, y):
        ...

    @alru_cache()
    async def another_called_alru_cached_instance_method(self, y):
        ...

    # aiocache

    @aiocache.cached
    async def aiocache_cached_instance_method(self, y):
        ...

    @cached
    async def another_aiocache_cached_instance_method(self, y):
        ...

    @aiocache.cached()
    async def called_aiocache_cached_instance_method(self, y):
        ...

    @cached()
    async def another_called_aiocache_cached_instance_method(self, y):
        ...

    @aiocache.cached_stampede
    async def aiocache_cached_stampede_instance_method(self, y):
        ...

    @cached_stampede
    async def another_aiocache_cached_stampede_instance_method(self, y):
        ...

    @aiocache.cached_stampede()
    async def called_aiocache_cached_stampede_instance_method(self, y):
        ...

    @cached_stampede()
    async def another_cached_stampede_aiocache_cached_instance_method(self, y):
        ...
    
    @aiocache.multi_cached
    async def aiocache_multi_cached_instance_method(self, y):
        ...

    @multi_cached
    async def another_aiocache_multi_cached_instance_method(self, y):
        ...

    @aiocache.multi_cached()
    async def called_aiocache_multi_cached_instance_method(self, y):
        ...

    @multi_cached()
    async def another_called_aiocache_multi_cached_instance_method(self, y):
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
    
    @async_lru.alru_cache
    async def alru_cached_instance_method_on_metaclass(cls, x: int):
        ...
    
    @aiocache.cached
    async def aiocache_cached_instance_method_on_metaclass(cls, x: int):
        ...
    
    @aiocache.cached_stampede
    async def aiocache_cached_stampede_instance_method_on_metaclass(cls, x: int):
        ...
    
    @aiocache.multi_cached
    async def aiocache_multi_cached_instance_method_on_metaclass(cls, x: int):
        ...
