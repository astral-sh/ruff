###
# Errors.
###
def f():
    global X


def f():
    global X

    print(X)


def f():
    global X

    if X > 0:
        del X


###
# Non-errors.
###
def f():
    global X

    X = 1


def f():
    global X

    (X, y) = (1, 2)


def f():
    global X

    del X


def f():
    global X

    X += 1
