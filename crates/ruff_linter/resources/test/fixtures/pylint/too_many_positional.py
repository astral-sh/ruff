def f(x, y, z, t, u, v, w, r):  # Too many positional arguments (8/5)
    pass


def f(x):  # OK
    pass


def f(x, y, z, _t, _u, _v, _w, r):  # OK (underscore-prefixed names are ignored
    pass


def f(x, y, z, *, u=1, v=1, r=1):  # OK
    pass


def f(x=1, y=1, z=1):  # OK
    pass


def f(x, y, z, /, u, v, w):  # Too many positional arguments (6/5)
    pass


def f(x, y, z, *, u, v, w):  # OK
    pass


def f(x, y, z, a, b, c, *, u, v, w):  # Too many positional arguments (6/5)
    pass


class C:
    def f(self, a, b, c, d, e):  # OK
        pass

    def f(self, /, a, b, c, d, e):  # OK
        pass

    def f(weird_self_name, a, b, c, d, e):  # OK
        pass

    def f(self, a, b, c, d, e, g):  # Too many positional arguments (6/5)
        pass

    @staticmethod
    def f(self, a, b, c, d, e):  # Too many positional arguments (6/5)
        pass

    @classmethod
    def f(cls, a, b, c, d, e):  # OK
        pass

    def f(*, self, a, b, c, d, e):  # OK
        pass

    def f(self=1, a=1, b=1, c=1, d=1, e=1):  # OK
        pass

    def f():  # OK
        pass
