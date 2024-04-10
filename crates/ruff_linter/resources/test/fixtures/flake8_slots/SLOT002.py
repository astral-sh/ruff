from collections import namedtuple
from typing import NamedTuple


class Bad(namedtuple("foo", ["str", "int"])):  # SLOT002
    pass


class UnusualButStillBad(NamedTuple("foo", [("x", int, "y", int)])):  # SLOT002
    pass


class UnusualButOkay(NamedTuple("foo", [("x", int, "y", int)])):
    __slots__ = ()


class Good(namedtuple("foo", ["str", "int"])):  # OK
    __slots__ = ("foo",)


class Good(NamedTuple):  # Ok
    pass
