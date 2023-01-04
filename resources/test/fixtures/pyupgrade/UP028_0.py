def f():
    for x in y:
        yield x


def g():
    for x, y in z:
        yield (x, y)


def h():
    for x in [1, 2, 3]:
        yield x


def i():
    for x in {x for x in y}:
        yield x


def j():
    for x in (1, 2, 3):
        yield x


def k():
    for x, y in {3: "x", 6: "y"}:
        yield x, y


def f():  # Comment one\n'
    # Comment two\n'
    for x, y in {  # Comment three\n'
        3: "x",  # Comment four\n'
        # Comment five\n'
        6: "y",  # Comment six\n'
    }:  # Comment seven\n'
        # Comment eight\n'
        yield x, y  # Comment nine\n'
        # Comment ten',


def f():
    for x, y in [{3: (3, [44, "long ss"]), 6: "y"}]:
        yield x, y


def f():
    for x, y in z():
        yield x, y

def f():
    def func():
        # This comment is preserved\n'
        for x, y in z():  # Comment one\n'
            # Comment two\n'
            yield x, y  # Comment three\n'
            # Comment four\n'
# Comment\n'
def g():
    print(3)


def f():
    for x in y:
        yield x
    for z in x:
        yield z


def f():
    for x, y in z():
        yield x, y
    x = 1
