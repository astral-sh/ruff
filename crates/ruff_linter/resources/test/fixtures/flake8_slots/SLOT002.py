from collections import namedtuple
from enum import Enum
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


class Good(namedtuple("foo", ["str", "int"]), Enum):
    pass


class UnusualButStillBad(namedtuple("foo", ["str", "int"]), NamedTuple("foo", [("x", int, "y", int)])):
    pass


class UnusualButStillBad(namedtuple("foo", ["str", "int"]), object):
    pass
