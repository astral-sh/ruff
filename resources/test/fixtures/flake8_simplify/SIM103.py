def f():
    if a:  # SIM103
        return True
    else:
        return False


def f():
    if a:  # OK
        foo()
        return True
    else:
        return False


def f():
    if a:  # OK
        return "foo"
    else:
        return False
