def x(y):
    if not y:
        return  # error
    return 1


# without return value
def x(y):
    if not y:
        return
    return


def x(y):
    if y:
        return
    print()


# with return value
def x(y):
    if not y:
        return 1
    return 2


def x(y):
    for i in range(10):
        if i > 100:
            return i
    else:
        return 1


def x(y):
    try:
        return 1
    except:
        return 2


def x(y):
    try:
        return 1
    finally:
        return 2


# inner function
def x(y):
    if not y:
        return 1

    def inner():
        return

    return 2


def x(y):
    if not y:
        return

    def inner():
        return 1
