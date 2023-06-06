from collections import namedtuple
from typing import NamedTuple


class Bad(namedtuple("foo", ["str", "int"])):  # SLOT002
    pass


class Good1(NamedTuple):  # Ok
    pass


class Good2(namedtuple("foo", ["str", "int"])):  # OK
    __slots__ = ("foo",)
