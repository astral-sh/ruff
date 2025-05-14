def f(x, y, z, t, u, v, w, r):  # Too many arguments (8/5)
    pass


def f(x, y, z, t, u):  # OK
    pass


def f(x):  # OK
    pass


def f(x, y, z, _t, _u, _v, _w, r):  # OK (underscore-prefixed names are ignored
    pass


def f(x, y, z, u=1, v=1, r=1):  # Too many arguments (6/5)
    pass


def f(x=1, y=1, z=1):  # OK
    pass


def f(x, y, z, /, u, v, w):  # Too many arguments (6/5)
    pass


def f(x, y, z, *, u, v, w):  # Too many arguments (6/5)
    pass


def f(x, y, z, a, b, c, *, u, v, w):  # Too many arguments (9/5)
    pass


from typing import override, overload


@override
def f(x, y, z, a, b, c, *, u, v, w):  # OK
    pass


@overload
def f(x, y, z, a, b, c, *, u, v, w):  # OK
    pass


class C:
    def f(self, y, z, a, b, c, *, u, v, w):  # Too many arguments (8/5)
        pass

    def f(self, y, z, a, b, c):  # OK
        pass

    @classmethod
    def f(cls, y, z, a, b, c, *, u, v, w):  # Too many arguments (8/5)
        pass

    @classmethod
    def f(cls, y, z, a, b, c):  # OK
        pass

    @staticmethod
    def f(y, z, a, b, c, *, u, v, w):  # Too many arguments (8/5)
        pass

    @staticmethod
    def f(y, z, a, b, c, d):  # OK
        pass

    @staticmethod
    def f(y, z, a, b, c):  # OK
        pass

class Foo:
    # `__new__` counts args like a classmethod
    # even though it is an implicit staticmethod
    def __new__(cls,a,b,c,d,e): # Ok
        ...
