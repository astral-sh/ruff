"""
Test for else-if-used
"""


def ok0():
    """Should not trigger on elif"""
    if 1:
        pass
    elif 2:
        pass


def ok1():
    """If the orelse has more than 1 item in it, shouldn't trigger"""
    if 1:
        pass
    else:
        print()
        if 1:
            pass


def ok2():
    """If the orelse has more than 1 item in it, shouldn't trigger"""
    if 1:
        pass
    else:
        if 1:
            pass
        print()


def not_ok0():
    if 1:
        pass
    else:
        if 2:
            pass


def not_ok1():
    if 1:
        pass
    else:
        if 2:
            pass
        else:
            pass
