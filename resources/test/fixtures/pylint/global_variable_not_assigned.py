###
# Errors.
###
def f():
    global x


def f():
    global x

    print(x)


###
# Non-errors.
###
def f():
    global x

    x = 1


def f():
    global x

    (x, y) = (1, 2)


def f():
    global x

    del x
