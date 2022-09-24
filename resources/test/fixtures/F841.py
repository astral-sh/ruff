try:
    1 / 0
except ValueError as e:
    pass


try:
    1 / 0
except ValueError as e:
    print(e)


def f1():
    x = 1
    y = 2
    z = x + y


def f2():
    foo = (1, 2)
    (a, b) = (1, 2)

    bar = (1, 2)
    (c, d) = bar

    (x, y) = baz = bar


def f3():
    locals()
    x = 1


def f4():
    _ = 1
    __ = 1
    _discarded = 1
