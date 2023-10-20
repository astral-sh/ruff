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


def f():
    def any(exp):
        pass

    for x in iterable:
        if check(x):
            return True
    return False


def f():
    def all(exp):
        pass

    for x in iterable:
        if check(x):
            return False
    return True


def f():
    x = 1

    # SIM110
    for x in iterable:
        if check(x):
            return True
    return False


def f():
    x = 1

    # SIM111
    for x in iterable:
        if check(x):
            return False
    return True


def f():
    # SIM110
    for x in "012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ":
        if x.isdigit():
            return True
    return False


def f():
    # OK (too long)
    for x in "012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9":
        if x.isdigit():
            return True
    return False

async def f():
    # OK
    for x in iterable:
        if await check(x):
            return True
    return False

async def f():
    # SIM110
    for x in iterable:
        if check(x):
            return True
    return False

async def f():
    # SIM110
    for x in await iterable:
        if check(x):
            return True
    return False

def f():
    # OK (can't turn this into any() because the yield would end up inside a genexp)
    for x in iterable:
        if (yield check(x)):
            return True
    return False

def f():
    # OK (same)
    for x in iterable:
        if (yield from check(x)):
            return True
    return False
