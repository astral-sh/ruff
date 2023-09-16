def bar():
    ...  # OK


def bar():
    pass  # OK


def bar():
    """oof"""  # OK


def oof():  # ERROR PYI048
    """oof"""
    print("foo")


def foo():  # ERROR PYI048
    """foo"""
    print("foo")
    print("foo")


def buzz():  # ERROR PYI048
    print("fizz")
    print("buzz")
    print("test")
