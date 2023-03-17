def bar():
    ...  # OK


def foo():
    """foo"""  # OK


def buzz():
    print("buzz")  # OK, not in stub file


def foo2():
    123  # OK, not in a stub file


def bizz():
    x = 123  # OK, not in a stub file
