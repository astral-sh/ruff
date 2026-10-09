from mymodule import *


def print_name():
    print(name)


def print_name(name):
    print(name)

__all__ = ['a']


class Foo:
    # OK: `sin` is imported explicitly below by the time the generator is consumed.
    values = (sin for _ in (0,))


from math import sin
