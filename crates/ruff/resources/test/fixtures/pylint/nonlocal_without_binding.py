nonlocal x


def f():
    nonlocal x


def f():
    nonlocal y


def f():
    x = 1

    def f():
        nonlocal x

    def f():
        nonlocal y
