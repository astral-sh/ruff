def bar():
    ...  # OK


def oof():
    """oof"""
    print("foo")  # OK, docstrings are handled by another rule


def foo():
    """foo"""
    print("foo")
    print("foo")  # Ok not in Stub file


def buzz():
    print("fizz")
    print("buzz")
    print("test")  # Ok not in Stub file
