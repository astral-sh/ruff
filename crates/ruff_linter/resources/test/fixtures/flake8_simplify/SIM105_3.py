"""Case: `contextlib` is imported after the call site."""


def foo():
    pass


def bar():
    # SIM105
    try:
        foo()
    except ValueError:
        pass


import contextlib
