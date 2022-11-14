import collections
from collections import namedtuple


class C:
    lower = 0
    CONSTANT = 0
    mixedCase = 0
    _mixedCase = 0
    mixed_Case = 0
    myObj1 = collections.namedtuple("MyObj", ["a", "b"])
    myObj2 = namedtuple("AnotherMyObj", ["a", "b"])
