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


def f():
    x = 1

    def g():
        nonlocal x

    del x


def f():
    def g():
        nonlocal x

    del x


def f():
    try:
        pass
    except Exception as x:
        pass

    def g():
        nonlocal x
        x = 2
