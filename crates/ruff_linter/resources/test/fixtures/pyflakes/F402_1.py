# Errors: the import is always executed before the loop.
from collections import abc


def f(xs, c):
    if c:
        for abc in xs:
            pass


def f(xs, c, d):
    if c:
        if d:
            for abc in xs:
                pass


def f(xs):
    try:
        for abc in xs:
            pass
    except ValueError:
        pass


# OK: the import and the loop are in different branches.
def f(xs, c):
    if c:
        import sys
    else:
        for sys in xs:
            pass


def f(xs):
    try:
        import json
    except ImportError:
        for json in xs:
            pass
