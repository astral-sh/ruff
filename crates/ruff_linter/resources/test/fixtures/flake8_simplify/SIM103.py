def f():
    # SIM103
    if a:
        return True
    else:
        return False


def f():
    # SIM103
    if a == b:
        return True
    else:
        return False


def f():
    # SIM103
    if a:
        return 1
    elif b:
        return True
    else:
        return False


def f():
    # SIM103
    if a:
        return 1
    else:
        if b:
            return True
        else:
            return False


def f():
    # OK
    if a:
        foo()
        return True
    else:
        return False


def f():
    # OK
    if a:
        return "foo"
    else:
        return False


def f():
    # SIM103
    if a:
        return False
    else:
        return True


def f():
    # OK
    if a:
        return False
    else:
        return False


def f():
    # OK
    if a:
        return True
    else:
        return True


def f():
    # SIM103 (but not fixable)
    def bool():
        return False
    if a:
        return True
    else:
        return False


def f():
    # SIM103
    if keys is not None and notice.key not in keys:
        return False
    else:
        return True


###
# Positive cases (preview)
###


def f():
    # SIM103
    if a:
        return True
    return False


def f():
    # SIM103
    if a:
        return False
    return True


def f():
    if not 10 < a:
        return False
    return True


def f():
    if 10 < a:
        return False
    return True


def f():
    if 10 in a:
        return False
    return True


def f():
    if 10 not in a:
        return False
    return True


def f():
    if a is 10:
        return False
    return True


def f():
    if a is not 10:
        return False
    return True


def f():
    if a == 10:
        return False
    return True


def f():
    if a != 10:
        return False
    return True


# https://github.com/astral-sh/ruff/issues/15323
# `and`/`or` chains of bool-returning operands should not be wrapped in `bool(...)`.
def f():
    if a == 0 and b == 0:
        return True
    else:
        return False


def f():
    if a == 0 or b == 0:
        return True
    else:
        return False


def f():
    if not a:
        return True
    else:
        return False
