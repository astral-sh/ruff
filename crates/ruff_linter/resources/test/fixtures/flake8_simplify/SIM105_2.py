"""Case: `contextlib` already imported."""
import contextlib


def foo():
    pass


# SIM105
try:
    foo()
except ValueError:
    pass
