def f(tup):
    x, y = tup


def f():
    x, y = 1, 2  # this triggers F841 as it's just a simple assignment where unpacking isn't needed


def f():
    (x, y) = coords = 1, 2
    if x > 1:
        print(coords)


def f():
    (x, y) = coords = 1, 2


def f():
    coords = (x, y) = 1, 2


def f():
    (a, b) = (x, y) = 1, 2  # this triggers F841 on everything
