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
