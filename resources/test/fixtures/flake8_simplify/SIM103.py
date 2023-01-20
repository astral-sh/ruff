def f():
    # SIM103
    if a:
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
    # SIM103 (but not fixable)
    if a:
        return False
    else:
        return True
