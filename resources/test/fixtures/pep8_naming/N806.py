import collections
from collections import namedtuple

GLOBAL: str = "foo"


def f():
    global GLOBAL
    GLOBAL = "bar"
    lower = 0
    Camel = 0
    CONSTANT = 0
    _ = 0
    MyObj1 = collections.namedtuple("MyObj1", ["a", "b"])
    MyObj2 = namedtuple("MyObj12", ["a", "b"])
