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


def f(x, y, z, /, u, v, w):  # OK
    pass


def f(x, y, z, *, u, v, w):  # OK
    pass


def f(x, y, z, a, b, c, *, u, v, w):  # Too many arguments (6/5)
    pass
