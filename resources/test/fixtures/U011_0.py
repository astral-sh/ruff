import functools
from functools import lru_cache


@lru_cache()
def fixme1():
    pass


@other_deco_after
@functools.lru_cache()
def fixme2():
    pass


@lru_cache(maxsize=None)
def fixme3():
    pass


@functools.lru_cache(maxsize=None)
@other_deco_before
def fixme4():
    pass


@lru_cache( # A 
) # B
def fixme5():
    pass


@lru_cache(
    # A
)   # B
def fixme6():
    pass


@functools.lru_cache(
    # A
    maxsize = None) # B
def fixme7():
    pass


@functools.lru_cache(
    # A1
    maxsize = None
    # A2
) # B
def fixme8():
    pass


@functools.lru_cache(
    # A1
    maxsize = 
    None
    # A2

)
def fixme9():
    pass


@functools.lru_cache(
    # A1
    maxsize = 
    None
    # A2
)
def fixme10():
    pass


@lru_cache
def correct1():
    pass


@functools.lru_cache
def correct2():
    pass


@functoools.lru_cache(maxsize=64)
def correct3():
    pass


def user_func():
    pass


@lru_cache(user_func)
def correct4():
    pass


@lru_cache(user_func, maxsize=None)
def correct5():
    pass
