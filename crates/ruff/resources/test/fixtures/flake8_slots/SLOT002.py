from collections import namedtuple
from typing import NamedTuple


class Bad1(namedtuple("foo", ["str", "int"])):  # SLOT002
    pass


named_tup = namedtuple("foo", ["str", "int"])


class Bad2(named_tup):  # SLOT002
    pass


class Good1(NamedTuple):  # Ok
    pass


class Good2(namedtuple("foo", ["str", "int"])):  # OK
    __slots__ = ("foo",)
