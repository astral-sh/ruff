import functools


def lru_cache(maxsize=None):
    pass


@lru_cache()
def dont_fixme():
    pass


@lru_cache(maxsize=None)
def dont_fixme():
    pass
