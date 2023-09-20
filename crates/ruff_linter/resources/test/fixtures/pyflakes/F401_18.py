"""Test that straight `__future__` imports are considered unused."""


def f():
    import __future__


def f():
    import __future__

    print(__future__.absolute_import)
