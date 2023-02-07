# These should NOT change
def f():
    for x in z:
        yield


def f():
    for x in z:
        yield y


def f():
    for x, y in z:
        yield x


def f():
    for x, y in z:
        yield y


def f():
    for a, b in z:
        yield x, y


def f():
    for x, y in z:
        yield y, x


def f():
    for x, y, c in z:
        yield x, y


def f():
    for x in z:
        x = 22
        yield x


def f():
    for x in z:
        yield x
    else:
        print("boom!")


def f():
    for x in range(5):
        yield x
    print(x)


def f():
    def g():
        print(x)

    for x in range(5):
        yield x
    g()


def f():
    def g():
        def h():
            print(x)

        return h

    for x in range(5):
        yield x
    g()()


def f(x):
    for x in y:
        yield x
    del x


async def f():
    for x in y:
        yield x


def f():
    x = 1
    print(x)
    for x in y:
        yield x


def f():
    for x in y:
        yield x
    print(x)


def f():
    for x in y:
        yield x
    z = lambda: x


def f():
    for x in y:
        yield x

    class C:
        def __init__(self):
            print(x)


def f():
    for x in y:
        yield x, x + 1


def f():
    for x, y in z:
        yield x, y, x + y
