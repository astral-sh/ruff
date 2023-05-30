def bar():    # OK
    ...


def oof():    # OK, docstrings are handled by another rule
    """oof"""
    print("foo")


def foo():    # Ok not in Stub file
    """foo"""
    print("foo")
    print("foo")


def buzz():    # Ok not in Stub file
    print("fizz")
    print("buzz")
    print("test")
