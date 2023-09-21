###
# Errors.
###
def f():
    print(x)

    global x

    print(x)


def f():
    global x

    print(x)

    global x

    print(x)


def f():
    print(x)

    global x, y

    print(x)


def f():
    global x, y

    print(x)

    global x, y

    print(x)


def f():
    x = 1

    global x

    x = 1


def f():
    global x

    x = 1

    global x

    x = 1


def f():
    del x

    global x, y

    del x


def f():
    global x, y

    del x

    global x, y

    del x


def f():
    del x

    global x

    del x


def f():
    global x

    del x

    global x

    del x


def f():
    del x

    global x, y

    del x


def f():
    global x, y

    del x

    global x, y

    del x


def f():
    print(f"{x=}")
    global x


###
# Non-errors.
###
def f():
    global x

    print(x)


def f():
    global x, y

    print(x)


def f():
    global x

    x = 1


def f():
    global x, y

    x = 1


def f():
    global x

    del x


def f():
    global x, y

    del x


def f():
    global x
    print(f"{x=}")
