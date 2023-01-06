def f():
    for x in iterable:  # SIM110
        if check(x):
            return True
    return False


def f():
    for el in [1, 2, 3]:
        if is_true(el):
            return True
    raise Exception


def f():
    for x in iterable:  # SIM111
        if check(x):
            return False
    return True


def f():
    for x in iterable:  # SIM 111
        if not x.is_empty():
            return False
    return True


def f():
    for x in iterable:
        if check(x):
            return "foo"
    return "bar"
