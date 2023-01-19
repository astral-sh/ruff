import functools
from functools import lru_cache


@functools.lru_cache(maxsize=None)
def fixme():
    pass


@lru_cache(maxsize=None)
def fixme():
    pass


@other_decorator
@functools.lru_cache(maxsize=None)
def fixme():
    pass


@functools.lru_cache(maxsize=None)
@other_decorator
def fixme():
    pass


@functools.lru_cache()
def ok():
    pass


@lru_cache()
def ok():
    pass


@functools.lru_cache(maxsize=64)
def ok():
    pass


@lru_cache(maxsize=64)
def ok():
    pass


def user_func():
    pass


@lru_cache(user_func)
def ok():
    pass


@lru_cache(user_func, maxsize=None)
def ok():
    pass


def lru_cache(maxsize=None):
    pass


@lru_cache(maxsize=None)
def ok():
    pass
