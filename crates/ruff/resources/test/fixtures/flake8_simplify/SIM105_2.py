"""Case: `contextlib` already imported."""
import contextlib


def foo():
    pass


try:
    foo()
except ValueError:
    pass
