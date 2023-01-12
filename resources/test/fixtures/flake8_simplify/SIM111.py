def f():
    # SIM110
    for x in iterable:
        if check(x):
            return True
    return False


def f():
    for x in iterable:
        if check(x):
            return True
    return True


def f():
    for el in [1, 2, 3]:
        if is_true(el):
            return True
    raise Exception


def f():
    # SIM111
    for x in iterable:
        if check(x):
            return False
    return True


def f():
    # SIM111
    for x in iterable:
        if not x.is_empty():
            return False
    return True


def f():
    for x in iterable:
        if check(x):
            return False
    return False


def f():
    for x in iterable:
        if check(x):
            return "foo"
    return "bar"


def f():
    # SIM110
    for x in iterable:
        if check(x):
            return True
    else:
        return False


def f():
    # SIM111
    for x in iterable:
        if check(x):
            return False
    else:
        return True


def f():
    # SIM110
    for x in iterable:
        if check(x):
            return True
    else:
        return False
    return True


def f():
    # SIM111
    for x in iterable:
        if check(x):
            return False
    else:
        return True
    return False


def f():
    for x in iterable:
        if check(x):
            return True
        elif x.is_empty():
            return True
    return False


def f():
    for x in iterable:
        if check(x):
            return True
        else:
            return True
    return False


def f():
    for x in iterable:
        if check(x):
            return True
        elif x.is_empty():
            return True
    else:
        return True
    return False
