import functools


@functools.lru_cache(maxsize=None)
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


@functools.lru_cache(maxsize=64)
def ok():
    pass


def user_func():
    pass


@functools.lru_cache(user_func)
def ok():
    pass


def lru_cache(maxsize=None):
    pass


@lru_cache(maxsize=None)
def ok():
    pass
