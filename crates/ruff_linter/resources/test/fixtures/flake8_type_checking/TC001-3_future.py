from collections import Counter

from elsewhere import third_party

from . import first_party


def f(x: first_party.foo): ...
def g(x: third_party.bar): ...
def h(x: Counter): ...
