"""Test case expected to be run with `suppress_none_returning = True`."""

# OK
def foo():
    a = 2 + 2


# OK
def foo():
    return


# OK
def foo():
    return None


# OK
def foo():
    a = 2 + 2
    if a == 4:
        return
    else:
        return


# OK
def foo():
    a = 2 + 2
    if a == 4:
        return None
    else:
        return


# OK
def foo():
    def bar() -> bool:
        return True

    bar()


# Error
def foo():
    return True


# Error
def foo():
    a = 2 + 2
    if a == 4:
        return True
    else:
        return
