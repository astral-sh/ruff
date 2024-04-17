def foo(a: list = []):
    raise NotImplementedError("")


def bar(a: dict = {}):
    """ This one also has a docstring"""
    raise NotImplementedError("and has some text in here")


def baz(a: list = []):
    """This one raises a different exception"""
    raise IndexError()
