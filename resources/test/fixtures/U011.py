
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

@lru_cache
def correct1():
    pass

@functools.lru_cache
def correct2():
    pass


