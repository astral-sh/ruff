try:
    1 / 0
except ValueError as e:
    pass


try:
    1 / 0
except ValueError as e:
    print(e)


def f():
    x = 1
    y = 2
    z = x + y


def g():
    foo = (1, 2)
    (a, b) = (1, 2)

    bar = (1, 2)
    (c, d) = bar

    (x, y) = baz = bar


def h():
    locals()
    x = 1
